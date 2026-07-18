use diffo_app::{ChangeArea, DiffViewMode, FileKey, Model, ToastKind};
use std::{
    env,
    sync::{
        Arc,
        mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    },
    thread,
    time::Duration,
};

use crossterm::event::{Event, MouseButton, MouseEventKind};
use diffo_core::{AccessMode, ChangeKind, FileState, RepositorySnapshot};
use diffo_diff::{
    DiffBlock, DiffDocument, ProjectionOptions, RenderLine, RowKind, SideBySideRow,
    inline_change_starts, inline_rows_with_options, parse_unified_patch,
    side_by_side_change_starts, side_by_side_rows_with_options,
};
use diffo_highlight::{HighlightedDiff, HighlightedLine, Rgb, StyledSpan, SyntaxHighlighter};
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
mod style;

#[cfg(test)]
use diff::{diff_file_lines, should_syntax_highlight};
use diff::{first_change, preparation_delay_from_environment, prepare_diff};
use files::{
    commit_action_at_position, file_group_areas, file_panel_areas, render_files, render_status,
    resize_border_style, staged_files, unstaged_files,
};
use geometry::{
    file_action_at_position, file_at_position, file_pane_percent_at, horizontal_panes,
    is_file_pane_splitter_at, main_area, overview_position, scrollbar_position_count,
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
use style::{
    file_action_style, file_kind_style, inline_line, network_animation_style, side_by_side_line,
};

pub use input::map_event;

pub struct Renderer {
    highlighter: Arc<SyntaxHighlighter>,
    highlighted: Option<HighlightCache>,
    prepare_tx: SyncSender<PrepareRequest>,
    prepare_rx: Receiver<PrepareOutcome>,
    submitted: Vec<DiffKey>,
    requested: Option<DiffKey>,
    failed: Option<DiffKey>,
    scrollbars: ScrollbarMetrics,
    scrollbar_drag: Option<ScrollbarAxis>,
    hunk_buttons: HunkButtonMetrics,
    hovered_hunk_button: Option<HunkDirection>,
    content_revision: u64,
    network_animation_tick: usize,
    #[cfg(test)]
    highlight_computations: usize,
}

struct HighlightCache {
    key: DiffKey,
    document: DiffDocument,
    inline: Vec<RenderLine>,
    side_by_side: Vec<SideBySideRow>,
    inline_changes: Vec<usize>,
    side_by_side_changes: Vec<usize>,
    highlighted: HighlightedDiff,
    #[cfg(test)]
    syntax_highlighted: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FramePreparation {
    pub maximum_vertical_scroll: usize,
    pub maximum_horizontal_scroll: usize,
    pub content_revision: u64,
    pub preparing: bool,
    pub viewport_transition: Option<ViewportTransition>,
    pub requested_file: Option<FileKey>,
    pub displayed_file: Option<FileKey>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ViewportTransition {
    pub vertical: usize,
    pub horizontal: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum AnchorRow {
    Inline {
        kind: RowKind,
        text: String,
    },
    SideBySide {
        old: Option<(RowKind, String)>,
        new: Option<(RowKind, String)>,
    },
}

struct ScrollAnchor {
    rows: Vec<(usize, usize, AnchorRow)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiffKey {
    file: FileKey,
    patch: String,
    mark_conflicts: bool,
}

struct PrepareRequest {
    key: DiffKey,
}

#[derive(Clone, Copy, Debug, Default)]
struct ScrollbarMetrics {
    vertical_area: Rect,
    horizontal_area: Rect,
    rows: usize,
    columns: usize,
    viewport_columns: usize,
    maximum_vertical_scroll: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct HunkButtonMetrics {
    previous: Option<(Rect, usize)>,
    next: Option<(Rect, usize)>,
}

#[derive(Clone, Copy, Debug)]
struct DiffViewportMetrics {
    content_area: Rect,
    horizontal_area: Rect,
    viewport_rows: usize,
    viewport_columns: usize,
    rows: usize,
    columns: usize,
    maximum_vertical_scroll: usize,
    previous_change: Option<usize>,
    next_change: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HunkDirection {
    Previous,
    Next,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScrollbarAxis {
    Vertical,
    Horizontal,
}

const MAX_HIGHLIGHT_FILE_LINES: usize = 10_000;
const MAX_SYNC_BYTES: usize = 64 * 1024;
const MAX_SYNC_LINES: usize = 500;

type PrepareOutcome = Result<HighlightCache, DiffKey>;

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    #[must_use]
    /// Create a renderer and its background diff worker.
    ///
    /// # Panics
    ///
    /// Panics if the operating system cannot start the worker thread.
    pub fn new() -> Self {
        let highlighter = Arc::new(SyntaxHighlighter::new());
        let worker_highlighter = Arc::clone(&highlighter);
        let (prepare_tx, requests) = sync_channel::<PrepareRequest>(1);
        let (results, prepare_rx) = sync_channel(1);
        let prepare_delay = preparation_delay_from_environment();
        thread::Builder::new()
            .name("diffo-diff-prepare".to_owned())
            .spawn(move || {
                while let Ok(request) = requests.recv() {
                    if !prepare_delay.is_zero() {
                        thread::sleep(prepare_delay);
                    }
                    let key = request.key.clone();
                    let result = prepare_diff(request, &worker_highlighter).ok_or(key);
                    if results.send(result).is_err() {
                        break;
                    }
                }
            })
            .expect("failed to start diff preparation worker");
        Self {
            highlighter,
            highlighted: None,
            prepare_tx,
            prepare_rx,
            submitted: Vec::new(),
            requested: None,
            failed: None,
            scrollbars: ScrollbarMetrics::default(),
            scrollbar_drag: None,
            hunk_buttons: HunkButtonMetrics::default(),
            hovered_hunk_button: None,
            content_revision: 0,
            network_animation_tick: 0,
            #[cfg(test)]
            highlight_computations: 0,
        }
    }

    pub fn render(&mut self, frame: &mut Frame, model: &Model) {
        if model.network_operation().is_some() {
            self.network_animation_tick = self.network_animation_tick.wrapping_add(1);
        } else {
            self.network_animation_tick = 0;
        }
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(frame.area());
        let panes = horizontal_panes(vertical[0], model.file_pane_percent);

        render_files(frame, panes[0], model);
        self.render_diff(frame, panes[1], model);
        render_status(frame, vertical[1], model, self.network_animation_tick);
        render_toasts(frame, model);
        render_command_palette(frame, model);
        render_help(frame, model);
        render_commit_editor(frame, model);
        render_file_context_menu(frame, model);
        if model.network_operation().is_some() {
            frame.render_widget(
                Block::default()
                    .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
                    .border_style(network_animation_style(self.network_animation_tick)),
                frame.area(),
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
            Some(DiffKey {
                file: selected.clone(),
                patch: diff.text.clone(),
                mark_conflicts: file.kind == ChangeKind::Conflicted,
            })
        });
        self.requested.clone_from(&requested);
        let displayed_before = self.displayed_key().cloned();
        let anchor = requested.as_ref().and_then(|requested| {
            self.highlighted
                .as_ref()
                .filter(|cache| cache.key.file == requested.file)
                .map(|cache| ScrollAnchor::capture(cache, model.diff_view_mode, model.diff_scroll))
        });
        let committed = self.prepare_requested(requested.as_ref());
        let displayed_after = self.displayed_key().cloned();
        let viewport_transition = committed.then(|| {
            let same_file = displayed_before
                .as_ref()
                .zip(displayed_after.as_ref())
                .is_some_and(|(before, after)| before.file == after.file);
            let vertical = if same_file {
                self.highlighted.as_ref().and_then(|cache| {
                    anchor
                        .and_then(|anchor| anchor.resolve(cache, model.diff_view_mode))
                        .or_else(|| first_change(cache, model.diff_view_mode))
                })
            } else {
                self.highlighted
                    .as_ref()
                    .and_then(|cache| first_change(cache, model.diff_view_mode))
            }
            .unwrap_or(0);
            ViewportTransition {
                vertical,
                horizontal: if same_file {
                    model.diff_horizontal_scroll
                } else {
                    0
                },
            }
        });
        let rendered_vertical_scroll = viewport_transition
            .map_or(model.diff_scroll, |viewport| viewport.vertical)
            .min(self.displayed_rows(model.diff_view_mode));
        let viewport =
            self.diff_viewport_metrics(model.diff_view_mode, diff_area, rendered_vertical_scroll);
        FramePreparation {
            maximum_vertical_scroll: viewport.maximum_vertical_scroll,
            maximum_horizontal_scroll: viewport.columns.saturating_sub(viewport.viewport_columns),
            content_revision: self.content_revision,
            preparing: self.requested.as_ref() != self.displayed_key(),
            viewport_transition,
            requested_file: self.requested.as_ref().map(|key| key.file.clone()),
            displayed_file: self.displayed_key().map(|key| key.file.clone()),
        }
    }

    #[must_use]
    pub fn is_preparing(&self) -> bool {
        self.requested.as_ref() != self.displayed_key()
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
                return Some(diffo_app::Message::SetDiffScroll(target));
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
                    return Some(diffo_app::Message::SetDiffScroll(change));
                }
                let axis = if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    self.scrollbar_at(mouse.column, mouse.row)
                } else {
                    self.scrollbar_drag
                };
                if let Some(axis) = axis {
                    self.scrollbar_drag = Some(axis);
                    return Some(self.scrollbar_message(axis, mouse.column, mouse.row));
                }
            }
        }
        match input::map_event(event, model, area) {
            Some(diffo_app::Message::JumpToPreviousChange) => self
                .change_jump(model, false)
                .map(diffo_app::Message::SetDiffScroll),
            Some(diffo_app::Message::JumpToNextChange) => self
                .change_jump(model, true)
                .map(diffo_app::Message::SetDiffScroll),
            message => message,
        }
    }
}

#[cfg(test)]
mod rendering_tests;
