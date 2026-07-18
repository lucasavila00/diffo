#![doc = include_str!("../README.md")]

use diffo_app::{ChangeArea, DiffViewMode, FileKey, Model, Toast, ToastKind};
use std::{
    env,
    sync::{
        Arc,
        mpsc::{channel, sync_channel},
    },
    thread,
    time::Duration,
};

use crossterm::event::{Event, MouseButton, MouseEventKind};
use diffo_core::{ChangeKind, FileState, HeadState, RepositorySnapshot};
use diffo_diff::{
    DiffBlock, DiffDocument, ProjectionOptions, RenderLine, RowKind, SideBySideRow,
    inline_change_starts, inline_rows_with_options, parse_unified_patch,
    side_by_side_change_starts, side_by_side_rows_with_options,
};
use diffo_file_picker::{Navigation as PickerNavigation, Outcome as PickerOutcome};
use diffo_highlight::{HighlightedDiff, HighlightedLine, Rgb, StyledSpan, SyntaxHighlighter};
use diffo_ui::{design, maximum_scroll, tool_areas};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table},
};

mod diff;
mod diff_view;
mod files;
mod geometry;
mod input;
mod overlays;
mod state;
mod style;

#[cfg(test)]
use diff::{diff_file_lines, should_syntax_highlight};
#[cfg(test)]
use diffo_ui::change_kind_style as file_kind_style;
#[cfg(test)]
use files::status_line;
use files::{
    commit_action_at_position, file_group_areas, file_panel_areas, picker_document,
    render_commit_composer, render_status, resize_border_style, staged_files, unstaged_files,
};
#[cfg(test)]
use geometry::scrollbar_position_count;
use geometry::{horizontal_panes, main_area, overview_position};
use overlays::{commit_editor_action_at_position, render_commit_editor, render_help};
pub use overlays::{render_toasts, toast_at_position};
#[cfg(test)]
use style::{
    contrast_ratio, contrasting_foreground, diff_background, diff_background_rgb, row_style,
};
use style::{
    inline_line, inline_skeleton_line, network_animation_style, side_by_side_line,
    side_by_side_skeleton_line,
};

use state::{
    AnchorRow, DiffKey, DiffViewportMetrics, HIGHLIGHT_PREFETCH_VIEWPORTS, HighlightCache,
    HunkButtonMetrics, MAX_SYNC_BYTES, MAX_SYNC_LINES, PREPARED_BUFFER_CACHE_SIZE, PrepareCommit,
    PrepareOutcome, PrepareRequest, ScrollAnchor, ScrollbarAxis, ScrollbarMetrics,
};

pub use diffo_highlight::{
    HIGHLIGHT_LOOKBEHIND_LINES, MAX_HIGHLIGHT_BYTES_PER_SIDE, MAX_HIGHLIGHT_FILE_LINES,
};
use diffo_text_view::{TextRenderMode, TextSurface, TextSurfacePreparation};
pub use diffo_ui::{change_kind_style, plain_syntax_spans, terminal_safe_text};
pub use state::{FramePreparation, Renderer, ViewportTransition};

pub use input::map_event;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RendererEvent {
    Consumed,
    Message(diffo_app::Message),
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

    pub fn render(&mut self, frame: &mut Frame, model: &Model) {
        self.render_in(frame, model, frame.area());
    }

    pub fn render_in(&mut self, frame: &mut Frame, model: &Model, area: Rect) {
        if model.network_operation().is_some() {
            self.network_animation_tick = self.network_animation_tick.wrapping_add(1);
        } else {
            self.network_animation_tick = 0;
        }
        let areas = tool_areas(area);
        let panes = horizontal_panes(areas.content, model.file_pane_percent);

        let file_panels = file_panel_areas(panes[0]);
        render_commit_composer(frame, file_panels[0], model);
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
        render_status(frame, areas.status, model, self.network_animation_tick);
        render_help(frame, model, area);
        render_commit_editor(frame, model, area);
        self.staged_picker.render_menu(frame);
        self.unstaged_picker.render_menu(frame);
        if model.network_operation().is_some() {
            frame.render_widget(
                Block::default()
                    .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
                    .border_style(network_animation_style(self.network_animation_tick)),
                area,
            );
        }
    }

    pub fn prepare_frame(&mut self, model: &Model, area: Rect) -> FramePreparation {
        let panes = horizontal_panes(main_area(area), model.file_pane_percent);
        self.prepare_file_pickers(model, panes[0]);
        let diff_area = panes[1];
        let requested = model.selected.as_ref().and_then(|selected| {
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
                patch,
                mark_conflicts: file.kind == ChangeKind::Conflicted,
                mode: model.diff_view_mode,
            })
        });
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
        self.diff_viewport_rows = usize::from(design::panel_content_extent(diff_area.height));
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
        let viewport =
            self.diff_viewport_metrics(displayed_mode, diff_area, rendered_vertical_scroll);
        let syntax_ready = self.failed.is_some()
            || self.syntax_ready_for_viewport(displayed_mode, rendered_vertical_scroll);
        FramePreparation {
            maximum_vertical_scroll: viewport.maximum_vertical_scroll,
            maximum_horizontal_scroll: maximum_scroll(viewport.columns, viewport.viewport_columns),
            content_revision: self.content_revision,
            preparing: self.requested.as_ref() != self.displayed_key(),
            syntax_ready,
            viewport_transition,
            requested_file: self.requested.as_ref().map(|key| key.file.clone()),
            displayed_file: self.displayed_key().map(|key| key.file.clone()),
            text_surface: Some(self.text_surface_preparation(
                rendered_vertical_scroll,
                syntax_ready,
                target_scroll,
                requested.as_ref(),
            )),
        }
    }

    fn prepare_file_pickers(&mut self, model: &Model, area: Rect) {
        let file_panels = file_panel_areas(area);
        let file_groups = file_group_areas(file_panels[1]);
        let border_style = resize_border_style(model);
        let selected = model.selected.as_ref();
        self.staged_picker.prepare(
            file_groups[0],
            picker_document(
                "Staged",
                "[-] Unstage All",
                staged_files(&model.snapshot),
                ChangeArea::Staged,
                border_style,
            ),
            selected.filter(|selected| selected.area == ChangeArea::Staged),
        );
        self.unstaged_picker.prepare(
            file_groups[1],
            picker_document(
                "Changes",
                "[+] Stage All",
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
        if model.help_open {
            return input::map_event(event, model, area).map(RendererEvent::Message);
        }
        if let Some(outcome) = self.map_picker_input(event, model, area) {
            return Some(outcome);
        }
        if let Event::Mouse(mouse) = event {
            if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                && let Some(target) = self.hunk_button_target_at(mouse.column, mouse.row)
            {
                self.requested_navigation_target = Some(target);
                return Some(RendererEvent::Message(
                    diffo_app::Message::JumpDiffToPosition(target),
                ));
            }
            if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
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
                        diffo_app::Message::JumpDiffToPosition(change),
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
        let message = match input::map_event(event, model, area) {
            Some(diffo_app::Message::JumpToPreviousChange) => {
                self.change_jump(model, false).map(|target| {
                    self.requested_navigation_target = Some(target);
                    diffo_app::Message::JumpDiffToPosition(target)
                })
            }
            Some(diffo_app::Message::JumpToNextChange) => {
                self.change_jump(model, true).map(|target| {
                    self.requested_navigation_target = Some(target);
                    diffo_app::Message::JumpDiffToPosition(target)
                })
            }
            message => message,
        }?;
        if matches!(
            message,
            diffo_app::Message::ScrollDiffUp
                | diffo_app::Message::ScrollDiffDown
                | diffo_app::Message::ScrollDiffPageUp(_)
                | diffo_app::Message::ScrollDiffPageDown(_)
                | diffo_app::Message::ScrollDiffVerticalBy(_)
                | diffo_app::Message::SetDiffScroll(_)
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
        if !model.commit_input_focused()
            && let Event::Key(key) = event
            && let Some(command) = diffo_file_picker::navigation(key)
        {
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
            RendererEvent::Message(diffo_app::Message::SelectFile(file))
        }
        PickerOutcome::RowAction(file) => RendererEvent::Message(match file.area {
            ChangeArea::Staged => diffo_app::Message::UnstageFile(file.path),
            ChangeArea::Unstaged => diffo_app::Message::StageFile(file.path),
        }),
        PickerOutcome::PanelAction => RendererEvent::Message(match area {
            ChangeArea::Staged => diffo_app::Message::UnstageAll,
            ChangeArea::Unstaged => diffo_app::Message::StageAll,
        }),
        PickerOutcome::CopyPath { id, absolute } => RendererEvent::CopyPath {
            path: id.path,
            absolute,
        },
    }
}

fn highlight_prefetch_viewports(previous: usize, current: usize, viewport_rows: usize) -> usize {
    if current < previous {
        return 4;
    }
    match current - previous {
        distance if distance >= viewport_rows => 12,
        1.. => 6,
        0 => HIGHLIGHT_PREFETCH_VIEWPORTS,
    }
}

#[cfg(test)]
mod rendering_tests;
