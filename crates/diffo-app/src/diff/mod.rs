//! Diff activity state, input, preparation, and rendering.

pub mod model;

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
use diffo_ui::file_picker::{Navigation as PickerNavigation, Outcome as PickerOutcome};
use diffo_ui::{design, maximum_scroll, tool_areas, wheel_scroll_delta};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

mod input;
mod prepare;
mod view;

pub(crate) use input::{help_rows, map_commit_event};
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
use view::geometry::scrollbar_position_count;
use view::geometry::{horizontal_panes, main_area, overview_position};
use view::overlays::commit_editor_action_at_position;
pub use view::overlays::{
    CommandProgress, command_cancel_at_position, render_command_progress, render_toasts,
    toast_at_position,
};
#[cfg(test)]
use view::style::{
    contrast_ratio, contrasting_foreground, diff_background, diff_background_rgb, row_style,
};
use view::style::{
    inline_line, inline_skeleton_line, raw_hunk_line, side_by_side_line, side_by_side_skeleton_line,
};

use prepare::state::{
    ChangeTarget, DiffKey, DiffViewportMetrics, HIGHLIGHT_PREFETCH_VIEWPORTS, HighlightCache,
    HunkButtonMetrics, MAX_SYNC_BYTES, MAX_SYNC_LINES, PREPARED_BUFFER_CACHE_SIZE, PrepareCommit,
    PrepareOutcome, PrepareRequest, ScrollAnchor, ScrollbarAxis, ScrollbarMetrics,
};

pub use diffo_highlight::{
    HIGHLIGHT_LOOKBEHIND_LINES, MAX_HIGHLIGHT_BYTES_PER_SIDE, MAX_HIGHLIGHT_FILE_LINES,
};
use diffo_ui::text_view::{LINE_SCROLL_ROWS, TextRenderMode, TextSurface, TextSurfacePreparation};
pub use diffo_ui::{change_kind_style, plain_syntax_spans, terminal_safe_text};
pub use prepare::state::{FramePreparation, Renderer, ViewportTransition};

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

impl Renderer {
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
        self.prepare_buffer(model, panes[1], false)
    }

    pub fn prepare_full_screen(&mut self, model: &Model, area: Rect) -> FramePreparation {
        self.prepare_buffer(model, area, true)
    }

    fn prepare_buffer(
        &mut self,
        model: &Model,
        diff_area: Rect,
        undecorated: bool,
    ) -> FramePreparation {
        let requested = self.requested_key(model);
        self.requested.clone_from(&requested);
        if self.requested_navigation_target.is_some() && requested.as_ref() != self.displayed_key()
        {
            self.requested_navigation_target = None;
        }
        let displayed_before = self.displayed_key().cloned();
        let anchor = requested.as_ref().and_then(|requested| {
            self.highlighted
                .as_ref()
                .filter(|cache| cache.key.file == requested.file)
                .map(|cache| ScrollAnchor::capture(cache, cache.key.mode, model.diff_scroll))
        });
        self.diff_viewport_rows = if undecorated {
            usize::from(diff_area.height)
        } else {
            usize::from(design::panel_content_extent(diff_area.height))
        };
        let prefetch_viewports = self.update_prefetch(model.diff_scroll);
        let target_scroll = self
            .navigation_preparation_target(requested.as_ref(), model.diff_view_mode)
            .or_else(|| {
                self.syntax_target(requested.as_ref(), model.diff_view_mode, model.diff_scroll)
            });
        let commit = self.prepare_requested(
            requested.as_ref(),
            self.diff_viewport_rows,
            model.diff_view_mode,
            target_scroll,
            prefetch_viewports,
        );
        let document_committed = commit
            .as_ref()
            .is_some_and(|commit| commit.target_scroll.is_none());
        let displayed_after = self.displayed_key().cloned();
        let navigation_transition = self.commit_ready_navigation(
            requested.as_ref(),
            model.diff_view_mode,
            model.diff_horizontal_scroll,
        );
        let viewport_transition = if navigation_transition.is_some() {
            navigation_transition
        } else {
            document_committed.then(|| {
                self.document_viewport_transition(
                    displayed_before.as_ref(),
                    displayed_after.as_ref(),
                    anchor.as_ref(),
                    model,
                )
            })
        };
        let rendered_vertical_scroll = viewport_transition
            .map_or(model.diff_scroll, |viewport| viewport.vertical)
            .min(self.displayed_rows(self.displayed_mode(model.diff_view_mode)));
        let displayed_mode = self.displayed_mode(model.diff_view_mode);
        let (maximum_vertical_scroll, maximum_horizontal_scroll) = if undecorated {
            let viewport = self.full_screen_metrics(diff_area, rendered_vertical_scroll);
            (viewport.maximum_vertical, viewport.maximum_horizontal)
        } else {
            let viewport =
                self.diff_viewport_metrics(displayed_mode, diff_area, rendered_vertical_scroll);
            (
                viewport.maximum_vertical_scroll,
                maximum_scroll(viewport.columns, viewport.viewport_columns),
            )
        };
        let syntax_ready = self.failed.is_some()
            || self.syntax_ready_for_viewport(displayed_mode, rendered_vertical_scroll);
        FramePreparation {
            maximum_vertical_scroll,
            maximum_horizontal_scroll,
            content_revision: self.content_revision,
            preparing: self.requested.as_ref() != self.displayed_key(),
            syntax_ready,
            viewport_transition,
            requested_file: self.requested.as_ref().map(|key| key.file.clone()),
            displayed_file: self.displayed_key().map(|key| key.file.clone()),
            requested_explorer_file: None,
            displayed_explorer_file: None,
            text_surface: Some(self.text_surface_preparation(
                rendered_vertical_scroll,
                syntax_ready,
                target_scroll,
                requested.as_ref(),
            )),
        }
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
            .filter(|key| key.file == *selected && key.patch.as_ref() == diff.text)
            .map_or_else(
                || Arc::<str>::from(diff.text.as_str()),
                |key| key.patch.clone(),
            );
        Some(DiffKey {
            file: selected.clone(),
            title: file_label(file),
            patch,
            mark_conflicts: file.kind == ChangeKind::Conflicted,
            mode: model.diff_view_mode,
        })
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
        if let Event::Mouse(mouse) = event {
            if mouse.kind == MouseEventKind::Up(MouseButton::Left) && self.scrollbar_drag.is_some()
            {
                self.scrollbar_drag = None;
                return Some(RendererEvent::Consumed);
            }
            if matches!(
                mouse.kind,
                MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
            ) {
                let axis = if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    self.scrollbar_at(mouse.column, mouse.row)
                } else {
                    self.scrollbar_drag
                };
                if let Some(axis) = axis {
                    self.scrollbar_drag = Some(axis);
                    self.requested_navigation_target = None;
                    let message = self.scrollbar_message(axis, mouse.column, mouse.row);
                    return Some(RendererEvent::Message(Self::vertical_message(
                        message, model,
                    )));
                }
            }
        }
        let page_rows = usize::from(
            area.height
                .saturating_sub(u16::from(!self.scrollbars.horizontal_area.is_empty())),
        )
        .max(1);
        let message = match event {
            Event::Key(key)
                if key.kind == KeyEventKind::Press && key.modifiers == KeyModifiers::NONE =>
            {
                match key.code {
                    KeyCode::Up => Message::ScrollDiffVerticalBy(-LINE_SCROLL_ROWS),
                    KeyCode::Down => Message::ScrollDiffVerticalBy(LINE_SCROLL_ROWS),
                    KeyCode::PageUp => Message::ScrollDiffPageUp(page_rows),
                    KeyCode::PageDown => Message::ScrollDiffPageDown(page_rows),
                    KeyCode::Left => Message::ScrollDiffHorizontalBy(-LINE_SCROLL_ROWS),
                    KeyCode::Right => Message::ScrollDiffHorizontalBy(LINE_SCROLL_ROWS),
                    KeyCode::Char('q') | KeyCode::Esc => Message::Quit,
                    _ => return None,
                }
            }
            Event::Key(key)
                if key.kind == KeyEventKind::Press
                    && key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                Message::Quit
            }
            Event::Mouse(mouse)
                if area.contains((mouse.column, mouse.row).into())
                    && wheel_scroll_delta(mouse.kind).is_some() =>
            {
                Message::ScrollDiffVerticalBy(wheel_scroll_delta(mouse.kind).unwrap_or_default())
            }
            _ => return None,
        };
        if matches!(
            message,
            Message::ScrollDiffPageUp(_)
                | Message::ScrollDiffPageDown(_)
                | Message::ScrollDiffVerticalBy(_)
        ) {
            self.requested_navigation_target = None;
        }
        Some(RendererEvent::Message(message))
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

    fn update_prefetch(&mut self, current_scroll: usize) -> usize {
        let viewports = highlight_prefetch_viewports(
            self.previous_diff_scroll,
            current_scroll,
            self.diff_viewport_rows,
        );
        self.previous_diff_scroll = current_scroll;
        viewports
    }

    #[must_use]
    pub fn is_preparing(&self) -> bool {
        self.requested.as_ref() != self.displayed_key() || !self.submitted.is_empty()
    }

    pub fn map_event(&mut self, event: &Event, model: &Model, area: Rect) -> Option<RendererEvent> {
        if let Some(outcome) = self.map_open_picker_menu(event, area) {
            return Some(outcome);
        }
        if let Some(outcome) = self.map_picker_input(event, model, area) {
            return Some(outcome);
        }
        let mut change_button_action = None;
        if let Event::Mouse(mouse) = event {
            if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                && let Some(next) = self.hunk_button_direction_at(mouse.column, mouse.row)
            {
                change_button_action = Some(if next {
                    crate::diff::Message::JumpToNextChange
                } else {
                    crate::diff::Message::JumpToPreviousChange
                });
            } else if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
                self.scrollbar_drag = None;
            } else if matches!(
                mouse.kind,
                MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
            ) {
                if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    && let Some(change) = self.change_at_marker(mouse.column, mouse.row, model)
                {
                    self.requested_navigation_target = Some(change);
                    return Some(RendererEvent::Message(
                        crate::diff::Message::JumpDiffToPosition(change),
                    ));
                }
                let axis = if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    self.scrollbar_at(mouse.column, mouse.row)
                } else {
                    self.scrollbar_drag
                };
                if let Some(axis) = axis {
                    self.scrollbar_drag = Some(axis);
                    let message = self.scrollbar_message(axis, mouse.column, mouse.row);
                    self.requested_navigation_target = None;
                    return Some(RendererEvent::Message(Self::vertical_message(
                        message, model,
                    )));
                }
            }
        }
        let message = match change_button_action.or_else(|| input::map_event(event, model, area)) {
            Some(crate::diff::Message::JumpToPreviousChange) => {
                self.change_jump(model, area, false).map(|target| {
                    self.requested_navigation_target = Some(target);
                    crate::diff::Message::JumpDiffToPosition(target)
                })
            }
            Some(crate::diff::Message::JumpToNextChange) => {
                self.change_jump(model, area, true).map(|target| {
                    self.requested_navigation_target = Some(target);
                    crate::diff::Message::JumpDiffToPosition(target)
                })
            }
            message => message,
        }?;
        if matches!(
            message,
            crate::diff::Message::ScrollDiffUp
                | crate::diff::Message::ScrollDiffDown
                | crate::diff::Message::ScrollDiffPageUp(_)
                | crate::diff::Message::ScrollDiffPageDown(_)
                | crate::diff::Message::ScrollDiffVerticalBy(_)
                | crate::diff::Message::SetDiffScroll(_)
        ) {
            self.requested_navigation_target = None;
        }
        Some(RendererEvent::Message(Self::vertical_message(
            message, model,
        )))
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
                .or(Some(RendererEvent::Consumed));
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

fn picker_event(outcome: PickerOutcome<FileKey>, area: ChangeArea) -> RendererEvent {
    match outcome {
        PickerOutcome::Consumed => RendererEvent::Consumed,
        PickerOutcome::Selected(file) | PickerOutcome::Activated(file) => {
            RendererEvent::Message(crate::diff::Message::SelectFile(file))
        }
        PickerOutcome::RowAction(file) => RendererEvent::Message(match file.area {
            ChangeArea::Staged => crate::diff::Message::UnstageFile(file.path),
            ChangeArea::Unstaged => crate::diff::Message::StageFile(file.path),
        }),
        PickerOutcome::PanelAction => RendererEvent::Message(match area {
            ChangeArea::Staged => crate::diff::Message::UnstageAll,
            ChangeArea::Unstaged => crate::diff::Message::StageAll,
        }),
        PickerOutcome::CopyPath { id, absolute } => RendererEvent::CopyPath {
            path: id.path,
            absolute,
        },
        PickerOutcome::DestructiveAction(file) => {
            RendererEvent::Message(crate::diff::Message::RequestDiscardFile(file.path))
        }
    }
}

fn highlight_prefetch_viewports(previous: usize, current: usize, viewport_rows: usize) -> usize {
    match current.abs_diff(previous) {
        distance if distance >= viewport_rows.max(1) => 13,
        1.. => 7,
        0 => HIGHLIGHT_PREFETCH_VIEWPORTS,
    }
}

#[cfg(test)]
mod rendering_tests;
