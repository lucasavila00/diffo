//! Diff activity state, input, preparation, and rendering.

pub mod model;

pub(crate) use model::MergePhase;
pub use model::{
    ChangeArea, DiffViewMode, Effect, FileKey, Message, Model, NetworkOperation, Toast, ToastKind,
    ToastQueue, update,
};
use std::{
    sync::{
        Arc,
        mpsc::{channel, sync_channel},
    },
    thread,
};

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_core::{ChangeKind, FileState, HeadState, RepositorySnapshot};
use diffo_diff::{
    ChangeRegion, DiffBlock, DiffDocument, ProjectionOptions, RenderLine, RowKind, SideBySideRow,
    inline_change_regions, inline_rows_with_options, parse_unified_patch,
    side_by_side_change_regions, side_by_side_rows_with_options,
};
use diffo_highlight::{HighlightedDiff, HighlightedLine, Rgb, StyledSpan, SyntaxHighlighter};
use diffo_ui::file_picker::Navigation as PickerNavigation;
use diffo_ui::{design, tool_areas};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

mod input;
mod prepare;
mod review;
mod review_document;
mod review_prepare;
mod view;

pub(crate) use input::{help_rows, map_commit_event, review_help_rows};
pub(crate) use view::ReviewRender;
pub(crate) use view::files::{FooterControl, footer_control_at_position, render_status};
pub(crate) use view::overlays::render_commit_editor;

#[cfg(test)]
#[cfg(test)]
use prepare::{diff_file_lines, should_syntax_highlight};
#[cfg(test)]
use view::files::status_line;
use view::files::{
    commit_action_at_position, file_group_areas, file_label, file_panel_areas, picker_document,
    render_commit_composer, render_unpushed_commits, resize_border_style, staged_files,
    unstaged_files,
};
#[cfg(test)]
use view::geometry::{diff_panel_inner, scrollbar_position_count};
use view::geometry::{horizontal_panes, main_area, overview_position};
use view::overlays::commit_editor_action_at_position;
pub use view::overlays::{
    CommandProgress, CommandProgressRow, CommandProgressState, command_at_position,
    render_command_progress, render_toasts, toast_at_position,
};
pub(crate) use view::style::raw_hunk_line;
#[cfg(test)]
use view::style::{
    contrast_ratio, contrasting_foreground, diff_background, diff_background_rgb, row_style,
};
use view::style::{
    inline_line, inline_skeleton_line, side_by_side_line, side_by_side_skeleton_line,
};

use prepare::state::{
    ChangeTarget, ChangeWarningAreas, DiffKey, DiffViewportMetrics, HighlightCache, HunkRow,
    MAX_SYNC_BYTES, MAX_SYNC_LINES, PREPARED_BUFFER_CACHE_SIZE, PrepareCommit, PrepareOutcome,
    PrepareRequest, ReviewPreparation, ScrollAnchor, ScrollbarAxis, ScrollbarMetrics,
};

pub use diffo_highlight::{
    HIGHLIGHT_LOOKBEHIND_LINES, MAX_HIGHLIGHT_BYTES_PER_SIDE, MAX_HIGHLIGHT_FILE_LINES,
};
use diffo_ui::text_view::{
    TextRenderMode, TextSurface, TextSurfacePreparation, syntax_prefetch_viewports,
};
pub use diffo_ui::{change_kind_style, plain_syntax_spans, terminal_safe_text};
use input::picker_event;
pub use prepare::state::{FramePreparation, Renderer, ViewportTransition};
pub(crate) use prepare::state::{
    ReviewDocument, ReviewHunkSegment, ReviewHunkSet, ReviewSelection,
};
pub use review::ReviewState;
use review_document::worktree_hunks;

pub use input::map_event;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RendererEvent {
    Consumed,
    Message(Message),
    CopyPath {
        path: std::path::PathBuf,
        absolute: bool,
    },
}

#[derive(Clone, Copy)]
struct ReviewFramePreparation {
    maximum_vertical: usize,
    maximum_horizontal: usize,
    syntax_ready: bool,
    viewport_transition: Option<ViewportTransition>,
    rendered_vertical: usize,
    target_scroll: Option<usize>,
}

impl Renderer {
    pub(crate) fn displayed_review_selection(&self) -> Option<&ReviewSelection> {
        self.displayed_selection.as_ref()
    }

    pub(crate) fn displayed_review_mode(&self) -> Option<DiffViewMode> {
        self.displayed_key().map(|key| key.mode)
    }

    #[must_use]
    pub fn has_open_picker_menu(&self) -> bool {
        self.staged_picker.has_open_menu() || self.unstaged_picker.has_open_menu()
    }

    pub fn dismiss_picker_menus(&mut self) {
        self.staged_picker.dismiss_menu();
        self.unstaged_picker.dismiss_menu();
    }

    pub fn render(&mut self, frame: &mut Frame, model: &Model) {
        self.render_in(frame, model, frame.area());
    }

    pub fn render_in(&mut self, frame: &mut Frame, model: &Model, area: Rect) {
        let areas = tool_areas(area);
        let panes = horizontal_panes(areas.content, model.file_pane_percent);

        let file_panels = file_panel_areas(panes[0]);
        render_commit_composer(frame, file_panels[0], model);
        let file_groups = file_group_areas(file_panels[1], model);
        render_unpushed_commits(frame, file_groups[0], model);
        self.staged_picker.render(
            frame,
            model
                .selected
                .as_ref()
                .is_some_and(|selected| selected.area == ChangeArea::Staged),
        );
        self.unstaged_picker.render(
            frame,
            model
                .selected
                .as_ref()
                .is_some_and(|selected| selected.area == ChangeArea::Unstaged),
        );
        self.render_diff(frame, panes[1], model);
        self.staged_picker.render_menu(frame);
        self.unstaged_picker.render_menu(frame);
    }

    pub fn prepare_frame(&mut self, model: &Model, area: Rect) -> FramePreparation {
        let panes = horizontal_panes(main_area(area), model.file_pane_percent);
        self.prepare_file_pickers(model, panes[0]);
        let requested = self.requested_key(model);
        let selection = model.selected.clone().map(ReviewSelection::File);
        self.prepare_buffer(ReviewPreparation {
            key: requested,
            selection,
            area: panes[1],
            undecorated: false,
            mode: model.diff_view_mode,
            vertical: model.diff_scroll,
            horizontal: model.diff_horizontal_scroll,
        })
    }

    pub fn prepare_full_screen(&mut self, model: &Model, area: Rect) -> FramePreparation {
        let requested = self.requested_key(model);
        let selection = model.selected.clone().map(ReviewSelection::File);
        self.prepare_buffer(ReviewPreparation {
            key: requested,
            selection,
            area,
            undecorated: true,
            mode: model.diff_view_mode,
            vertical: model.diff_scroll,
            horizontal: model.diff_horizontal_scroll,
        })
    }

    pub(crate) fn prepare_review(
        &mut self,
        document: Option<&ReviewDocument>,
        area: Rect,
        undecorated: bool,
        mode: DiffViewMode,
        vertical: usize,
        horizontal: usize,
    ) -> FramePreparation {
        self.prepare_buffer(ReviewPreparation {
            key: document.map(|document| document.key(mode)),
            selection: document.map(|document| document.selection.clone()),
            area,
            undecorated,
            mode,
            vertical,
            horizontal,
        })
    }

    fn prepare_buffer(&mut self, review: ReviewPreparation) -> FramePreparation {
        let ReviewPreparation {
            key: requested,
            selection: requested_selection,
            area: diff_area,
            undecorated,
            mode: requested_mode,
            vertical,
            horizontal,
        } = review;
        self.requested.clone_from(&requested);
        self.requested_selection.clone_from(&requested_selection);
        let focus_changed = self.requested_selection != self.displayed_selection;
        if focus_changed {
            self.vertical_scroll.clear();
        }
        if self.vertical_scroll.requested().is_some() && requested.as_ref() != self.displayed_key()
        {
            self.vertical_scroll.clear();
        }
        let displayed_before = self.displayed_key().cloned();
        let anchor = requested.as_ref().and_then(|requested| {
            self.highlighted
                .as_ref()
                .filter(|cache| cache.key.selection == requested.selection)
                .map(|cache| ScrollAnchor::capture(cache, cache.key.mode, vertical))
        });
        self.diff_viewport_rows = if undecorated {
            usize::from(diff_area.height)
        } else {
            usize::from(design::panel_content_extent(diff_area.height))
        };
        let target_scroll = self
            .navigation_preparation_target(requested.as_ref(), requested_mode)
            .or_else(|| self.syntax_target(requested.as_ref(), requested_mode, vertical));
        let prefetch_viewports = syntax_prefetch_viewports(
            vertical,
            target_scroll.unwrap_or(vertical),
            self.diff_viewport_rows,
        );
        let commit = self.prepare_requested(
            requested.as_ref(),
            self.diff_viewport_rows,
            requested_mode,
            target_scroll,
            prefetch_viewports,
        );
        let document_committed = commit
            .as_ref()
            .is_some_and(|commit| commit.target_scroll.is_none());
        let displayed_after = self.displayed_key().cloned();
        let focus_transition = self.focus_transition(focus_changed, requested_selection.as_ref());
        let navigation_transition =
            self.commit_ready_navigation(requested.as_ref(), requested_mode, horizontal);
        let viewport_transition = if focus_transition.is_some() {
            focus_transition
        } else if navigation_transition.is_some() {
            navigation_transition
        } else {
            document_committed.then(|| {
                self.document_viewport_transition(
                    displayed_before.as_ref(),
                    displayed_after.as_ref(),
                    anchor.as_ref(),
                    horizontal,
                )
            })
        };
        if (document_committed || focus_transition.is_some())
            && self.requested.as_ref() == self.displayed_key()
        {
            self.displayed_selection.clone_from(&requested_selection);
        }
        if requested.is_none() {
            self.displayed_selection = None;
        }
        let rendered_vertical_scroll = viewport_transition
            .map_or(vertical, |viewport| viewport.vertical)
            .min(self.displayed_rows(self.displayed_mode(requested_mode)));
        let displayed_mode = self.displayed_mode(requested_mode);
        let (maximum_vertical, maximum_horizontal) = self.review_scroll_bounds(
            diff_area,
            undecorated,
            displayed_mode,
            rendered_vertical_scroll,
        );
        let syntax_ready = self.failed.is_some()
            || self.syntax_ready_for_viewport(displayed_mode, rendered_vertical_scroll);
        self.frame_preparation(
            requested.as_ref(),
            ReviewFramePreparation {
                maximum_vertical,
                maximum_horizontal,
                syntax_ready,
                viewport_transition,
                rendered_vertical: rendered_vertical_scroll,
                target_scroll,
            },
        )
    }

    fn requested_key(&self, model: &Model) -> Option<DiffKey> {
        let selected = model.selected.as_ref()?;
        let file = model
            .snapshot
            .files
            .iter()
            .find(|file| file.path == selected.path)?;
        let diff = match selected.area {
            ChangeArea::Unstaged => file.unstaged.as_ref(),
            ChangeArea::Staged => file.staged.as_ref(),
        }?;
        let patch = self
            .requested
            .as_ref()
            .filter(|key| {
                key.selection.file_key() == Some(selected) && key.patch.as_ref() == diff.text
            })
            .map_or_else(
                || Arc::<str>::from(diff.text.as_str()),
                |key| key.patch.clone(),
            );
        let document = ReviewDocument {
            selection: ReviewSelection::File(selected.clone()),
            title: file_label(file),
            patch,
            mark_conflicts: file.kind == ChangeKind::Conflicted,
            hunks: worktree_hunks(model),
        };
        Some(document.key(model.diff_view_mode))
    }

    #[must_use]
    pub fn full_screen_title(&self) -> Option<Line<'static>> {
        self.displayed_key().map(|key| key.title.clone())
    }

    pub fn map_full_screen_event(
        &mut self,
        event: &Event,
        model: &Model,
        area: Rect,
    ) -> Option<RendererEvent> {
        if let Some(event) = self.map_review_event(event, &model.review, area) {
            return Some(event);
        }
        let Event::Key(key) = event else {
            return None;
        };
        (key.kind == KeyEventKind::Press
            && (matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                || (key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL))))
        .then_some(RendererEvent::Message(Message::Quit))
    }

    pub(crate) fn map_review_event(
        &mut self,
        event: &Event,
        state: &ReviewState,
        area: Rect,
    ) -> Option<RendererEvent> {
        if let Event::Mouse(mouse) = event {
            if mouse.kind == MouseEventKind::Up(MouseButton::Left)
                && self.scrollbar_drag.take().is_some()
            {
                return Some(RendererEvent::Consumed);
            }
            if matches!(
                mouse.kind,
                MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
            ) {
                if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    let position = (mouse.column, mouse.row).into();
                    let next = if self
                        .change_warnings
                        .previous
                        .is_some_and(|target| target.contains(position))
                    {
                        Some(false)
                    } else if self
                        .change_warnings
                        .next
                        .is_some_and(|target| target.contains(position))
                    {
                        Some(true)
                    } else {
                        None
                    };
                    if let Some(next) = next
                        && let Some(target) = self.review_change_jump(
                            state.diff_view_mode,
                            area,
                            state.diff_scroll,
                            next,
                        )
                    {
                        return Some(RendererEvent::Message(
                            self.vertical_message(Message::SetDiffScroll(target), state),
                        ));
                    }
                    if let Some(target) = self.change_at_marker(mouse.column, mouse.row) {
                        return Some(RendererEvent::Message(
                            self.vertical_message(Message::SetDiffScroll(target), state),
                        ));
                    }
                }
                let axis = if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    self.scrollbar_at(mouse.column, mouse.row)
                } else {
                    self.scrollbar_drag
                };
                if let Some(axis) = axis {
                    self.scrollbar_drag = Some(axis);
                    let message = self.scrollbar_message(axis, mouse.column, mouse.row);
                    return Some(RendererEvent::Message(
                        self.vertical_message(message, state),
                    ));
                }
            }
        }
        let message = match input::map_review_event(event, area)? {
            Message::ScrollDiffPageUp(_) => Message::ScrollDiffPageUp(self.diff_viewport_rows),
            Message::ScrollDiffPageDown(_) => Message::ScrollDiffPageDown(self.diff_viewport_rows),
            Message::JumpToPreviousChange => {
                let target =
                    self.review_change_jump(state.diff_view_mode, area, state.diff_scroll, false)?;
                Message::SetDiffScroll(target)
            }
            Message::JumpToNextChange => {
                let target =
                    self.review_change_jump(state.diff_view_mode, area, state.diff_scroll, true)?;
                Message::SetDiffScroll(target)
            }
            message => message,
        };
        Some(RendererEvent::Message(
            self.vertical_message(message, state),
        ))
    }

    fn prepare_file_pickers(&mut self, model: &Model, area: Rect) {
        let file_panels = file_panel_areas(area);
        let file_groups = file_group_areas(file_panels[1], model);
        let border_style = resize_border_style(model);
        let selected = model.selected.as_ref();
        self.staged_picker.prepare(
            file_groups[1],
            picker_document(
                "Staged",
                " Unstage All",
                staged_files(&model.snapshot),
                ChangeArea::Staged,
                border_style,
            ),
            selected.filter(|selected| selected.area == ChangeArea::Staged),
        );
        self.unstaged_picker.prepare(
            file_groups[2],
            picker_document(
                "Changes",
                " Stage All",
                unstaged_files(&model.snapshot),
                ChangeArea::Unstaged,
                border_style,
            ),
            selected.filter(|selected| selected.area == ChangeArea::Unstaged),
        );
    }

    fn text_surface_preparation(
        &self,
        scroll: usize,
        syntax_ready: bool,
        target_scroll: Option<usize>,
        requested: Option<&DiffKey>,
    ) -> TextSurfacePreparation {
        let coverage = self.highlighted.as_ref().and_then(|cache| {
            cache
                .highlighted_new_coverage
                .last()
                .map(|range| (range.start, range.end))
        });
        TextSurfacePreparation {
            surface: TextSurface::Diff,
            document_revision: self.content_revision,
            viewport: (scroll, self.diff_viewport_rows),
            requested_range: (scroll, scroll.saturating_add(self.diff_viewport_rows)),
            mode: if self.requested.as_ref() != self.displayed_key() {
                TextRenderMode::TextSkeleton
            } else if syntax_ready {
                TextRenderMode::Full
            } else {
                TextRenderMode::SyntaxSkeleton
            },
            coverage_before: coverage,
            coverage_after: coverage,
            request_id: None,
            cache_hit: target_scroll.is_none() && requested == self.displayed_key(),
            coalesced_request: false,
            stale_discarded: false,
        }
    }

    #[must_use]
    pub fn is_preparing(&self) -> bool {
        self.requested.as_ref() != self.displayed_key()
            || self.requested_selection != self.displayed_selection
            || !self.submitted.is_empty()
    }

    pub fn map_event(&mut self, event: &Event, model: &Model, area: Rect) -> Option<RendererEvent> {
        if let Some(outcome) = self.map_open_picker_menu(event, area) {
            return Some(outcome);
        }
        if let Some(outcome) = self.map_picker_input(event, model, area) {
            return Some(outcome);
        }
        let review_area = horizontal_panes(main_area(area), model.file_pane_percent)[1];
        self.map_review_event(event, &model.review, review_area)
            .or_else(|| input::map_event(event, model, area).map(RendererEvent::Message))
    }

    fn map_open_picker_menu(&mut self, event: &Event, area: Rect) -> Option<RendererEvent> {
        if self.staged_picker.has_open_menu() {
            return self
                .staged_picker
                .handle_event(event, area)
                .map(|outcome| picker_event(outcome, ChangeArea::Staged))
                .or(Some(RendererEvent::Consumed));
        }
        self.unstaged_picker.has_open_menu().then(|| {
            self.unstaged_picker
                .handle_event(event, area)
                .map_or(RendererEvent::Consumed, |outcome| {
                    picker_event(outcome, ChangeArea::Unstaged)
                })
        })
    }

    fn map_picker_input(
        &mut self,
        event: &Event,
        model: &Model,
        area: Rect,
    ) -> Option<RendererEvent> {
        if let Event::Key(key) = event
            && let Some(command) = diffo_ui::file_picker::navigation(key)
        {
            if command == PickerNavigation::Activate {
                return None;
            }
            return self
                .navigate_file_pickers(command, model)
                .filter(|event| match event {
                    RendererEvent::Message(crate::diff::Message::SelectFile(file)) => {
                        model.selected.as_ref() != Some(file)
                    }
                    RendererEvent::Consumed
                    | RendererEvent::Message(_)
                    | RendererEvent::CopyPath { .. } => true,
                });
        }
        let Event::Mouse(_) = event else {
            return None;
        };
        self.staged_picker
            .handle_event(event, area)
            .map(|outcome| picker_event(outcome, ChangeArea::Staged))
            .or_else(|| {
                self.unstaged_picker
                    .handle_event(event, area)
                    .map(|outcome| picker_event(outcome, ChangeArea::Unstaged))
            })
    }

    fn navigate_file_pickers(
        &mut self,
        command: PickerNavigation,
        model: &Model,
    ) -> Option<RendererEvent> {
        let current_area = model.selected.as_ref().map(|selected| selected.area);
        let (outcome, area) = match command {
            PickerNavigation::First => self
                .staged_picker
                .navigate(command)
                .map(|outcome| (outcome, ChangeArea::Staged))
                .or_else(|| {
                    self.unstaged_picker
                        .navigate(command)
                        .map(|outcome| (outcome, ChangeArea::Unstaged))
                })?,
            PickerNavigation::Last => self
                .unstaged_picker
                .navigate(command)
                .map(|outcome| (outcome, ChangeArea::Unstaged))
                .or_else(|| {
                    self.staged_picker
                        .navigate(command)
                        .map(|outcome| (outcome, ChangeArea::Staged))
                })?,
            PickerNavigation::Previous if current_area == Some(ChangeArea::Unstaged) => {
                let before = self.unstaged_picker.selected().cloned();
                let outcome = self.unstaged_picker.navigate(command);
                if outcome.is_none() || self.unstaged_picker.selected() == before.as_ref() {
                    self.staged_picker
                        .navigate(PickerNavigation::Last)
                        .map(|outcome| (outcome, ChangeArea::Staged))
                        .or_else(|| outcome.map(|outcome| (outcome, ChangeArea::Unstaged)))?
                } else {
                    (outcome?, ChangeArea::Unstaged)
                }
            }
            PickerNavigation::Next if current_area == Some(ChangeArea::Staged) => {
                let before = self.staged_picker.selected().cloned();
                let outcome = self.staged_picker.navigate(command);
                if outcome.is_none() || self.staged_picker.selected() == before.as_ref() {
                    self.unstaged_picker
                        .navigate(PickerNavigation::First)
                        .map(|outcome| (outcome, ChangeArea::Unstaged))
                        .or_else(|| outcome.map(|outcome| (outcome, ChangeArea::Staged)))?
                } else {
                    (outcome?, ChangeArea::Staged)
                }
            }
            _ => {
                let area = current_area?;
                let outcome = match area {
                    ChangeArea::Staged => self.staged_picker.navigate(command),
                    ChangeArea::Unstaged => self.unstaged_picker.navigate(command),
                }?;
                (outcome, area)
            }
        };
        Some(picker_event(outcome, area))
    }
}

#[cfg(test)]
mod rendering_tests;
