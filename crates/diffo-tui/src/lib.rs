use diffo_app::{ChangeArea, DiffViewMode, FileKey, FileListScroll, Model, ToastKind};
use std::{
    env,
    sync::{
        Arc,
        mpsc::{TrySendError, sync_channel},
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
use diffo_highlight::{HighlightedDiff, HighlightedLine, Rgb, StyledSpan, SyntaxHighlighter};
use diffo_ui::tool_areas;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph, Row,
        Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
    },
};

mod diff;
mod diff_view;
mod files;
mod geometry;
mod input;
mod overlays;
mod state;
mod style;

use diff::first_change;
#[cfg(test)]
use diff::{diff_file_lines, should_syntax_highlight};
#[cfg(test)]
use diffo_ui::change_kind_style as file_kind_style;
#[cfg(test)]
use files::status_line;
use files::{
    FileListMetrics, commit_action_at_position, file_group_areas, file_group_metrics,
    file_panel_areas, prepared_file_list_scroll, render_files, render_status, resize_border_style,
    staged_files, unstaged_files,
};
use geometry::{
    file_action_at_position, file_at_position, file_group_at_position, file_pane_percent_at,
    horizontal_panes, is_file_pane_splitter_at, main_area, overview_position,
    scrollbar_position_count,
};
use overlays::{
    command_palette_layout, commit_editor_action_at_position, map_file_context_menu_event,
    render_command_palette, render_commit_editor, render_file_context_menu, render_help,
    render_toasts, toast_at_position,
};
#[cfg(test)]
use style::{
    contrast_ratio, contrasting_foreground, diff_background, diff_background_rgb, row_style,
};
use style::{file_action_style, inline_line, network_animation_style, side_by_side_line};

use state::{
    AnchorRow, DiffKey, DiffViewportMetrics, HIGHLIGHT_PREFETCH_VIEWPORTS, HighlightCache,
    HunkButtonMetrics, HunkDirection, MAX_SYNC_BYTES, MAX_SYNC_LINES, PREPARED_BUFFER_CACHE_SIZE,
    PrepareCommit, PrepareOutcome, PrepareRequest, ScrollAnchor, ScrollbarAxis, ScrollbarMetrics,
};

pub use diffo_highlight::{
    HIGHLIGHT_LOOKBEHIND_LINES, MAX_HIGHLIGHT_BYTES_PER_SIDE, MAX_HIGHLIGHT_FILE_LINES,
};
pub use diffo_ui::{change_kind_style, plain_syntax_spans};
pub use state::{FramePreparation, Renderer, ViewportTransition};

pub use input::map_event;

impl Renderer {
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

        self.file_lists = render_files(frame, panes[0], model);
        self.render_diff(frame, panes[1], model);
        render_status(frame, areas.status, model, self.network_animation_tick);
        render_toasts(frame, model, area);
        render_command_palette(frame, model, area);
        render_help(frame, model, area);
        render_commit_editor(frame, model, area);
        render_file_context_menu(frame, model, area);
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
        let diff_area = horizontal_panes(main_area(area), model.file_pane_percent)[1];
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
        if self.requested.as_ref() != requested.as_ref() {
            self.pending_scroll = None;
        }
        self.requested.clone_from(&requested);
        let displayed_before = self.displayed_key().cloned();
        let anchor = requested.as_ref().and_then(|requested| {
            self.highlighted
                .as_ref()
                .filter(|cache| cache.key.file == requested.file)
                .map(|cache| ScrollAnchor::capture(cache, cache.key.mode, model.diff_scroll))
        });
        self.diff_viewport_rows = usize::from(diff_area.height.saturating_sub(2));
        let commit = self.prepare_requested(
            requested.as_ref(),
            self.diff_viewport_rows,
            model.diff_view_mode,
        );
        let committed = commit.is_some();
        let displayed_after = self.displayed_key().cloned();
        let viewport_transition = committed.then(|| {
            let same_file = displayed_before
                .as_ref()
                .zip(displayed_after.as_ref())
                .is_some_and(|(before, after)| before.file == after.file);
            let same_mode = displayed_before
                .as_ref()
                .zip(displayed_after.as_ref())
                .is_some_and(|(before, after)| before.mode == after.mode);
            let vertical = if let Some(target) = commit.and_then(|commit| commit.target_scroll) {
                Some(target)
            } else if same_file && same_mode {
                self.highlighted.as_ref().and_then(|cache| {
                    anchor
                        .and_then(|anchor| anchor.resolve(cache, cache.key.mode))
                        .or_else(|| first_change(cache, cache.key.mode))
                })
            } else if same_file {
                Some(0)
            } else {
                self.highlighted
                    .as_ref()
                    .and_then(|cache| first_change(cache, cache.key.mode))
            }
            .unwrap_or(0);
            ViewportTransition {
                vertical,
                horizontal: if same_file && same_mode {
                    model.diff_horizontal_scroll
                } else {
                    0
                },
            }
        });
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
            maximum_horizontal_scroll: viewport.columns.saturating_sub(viewport.viewport_columns),
            content_revision: self.content_revision,
            preparing: self.requested.as_ref() != self.displayed_key(),
            syntax_ready,
            viewport_transition,
            requested_file: self.requested.as_ref().map(|key| key.file.clone()),
            displayed_file: self.displayed_key().map(|key| key.file.clone()),
            file_list_scroll: prepared_file_list_scroll(model, area),
        }
    }

    #[must_use]
    pub fn is_preparing(&self) -> bool {
        self.requested.as_ref() != self.displayed_key() || self.pending_scroll.is_some()
    }

    pub fn map_event(
        &mut self,
        event: &Event,
        model: &Model,
        area: Rect,
    ) -> Option<diffo_app::Message> {
        if model.file_context_menu.is_some() {
            return map_file_context_menu_event(event, model, area);
        }
        if !model.commit_input_focused()
            && model.command_palette.is_none()
            && !model.help_open
            && let Event::Mouse(mouse) = event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && let Some(id) = toast_at_position(model, area, mouse.column, mouse.row)
        {
            return Some(diffo_app::Message::DismissToast(id));
        }
        if model.command_palette.is_some() || model.help_open {
            if let Event::Mouse(mouse) = event
                && mouse.kind == MouseEventKind::Down(MouseButton::Left)
            {
                let (_, results_area) = command_palette_layout(area);
                let match_count = model
                    .command_palette
                    .as_ref()
                    .map_or(0, |palette| palette.matches().len());
                if results_area.contains((mouse.column, mouse.row).into()) {
                    let index = usize::from(mouse.row.saturating_sub(results_area.y));
                    if index < match_count {
                        return Some(diffo_app::Message::ExecuteCommand(index));
                    }
                }
            }
            return input::map_event(event, model, area);
        }
        if let Event::Mouse(mouse) = event {
            self.hovered_hunk_button = self.hunk_button_at(mouse.column, mouse.row);
            if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                && let Some(target) = self.hunk_button_target_at(mouse.column, mouse.row)
            {
                return self
                    .guard_vertical_message(diffo_app::Message::SetDiffScroll(target), model);
            }
            if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
                self.scrollbar_drag = None;
                self.file_scrollbar_drag = None;
            } else if matches!(
                mouse.kind,
                MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
            ) {
                let file_area = if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    let area = self.file_scrollbar_at(mouse.column, mouse.row);
                    self.file_scrollbar_drag = area;
                    area
                } else {
                    self.file_scrollbar_drag
                };
                if let Some(area) = file_area {
                    self.file_scrollbar_drag = Some(area);
                    self.scrollbar_drag = None;
                    return Some(self.file_scrollbar_message(area, mouse.row));
                }
                if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    && let Some(change) = self.change_at_marker(mouse.column, mouse.row, model)
                {
                    return self
                        .guard_vertical_message(diffo_app::Message::SetDiffScroll(change), model);
                }
                let axis = if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    self.scrollbar_at(mouse.column, mouse.row)
                } else {
                    self.scrollbar_drag
                };
                if let Some(axis) = axis {
                    self.scrollbar_drag = Some(axis);
                    let message = self.scrollbar_message(axis, mouse.column, mouse.row);
                    return self.guard_vertical_message(message, model);
                }
            }
        }
        let message = match input::map_event(event, model, area) {
            Some(diffo_app::Message::JumpToPreviousChange) => self
                .change_jump(model, false)
                .map(diffo_app::Message::SetDiffScroll),
            Some(diffo_app::Message::JumpToNextChange) => self
                .change_jump(model, true)
                .map(diffo_app::Message::SetDiffScroll),
            message => message,
        }?;
        self.guard_vertical_message(message, model)
    }
}

#[cfg(test)]
mod rendering_tests;
