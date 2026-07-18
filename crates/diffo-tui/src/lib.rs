use diffo_app::{ChangeArea, DiffViewMode, FileKey, Model, ToastKind};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    },
    thread,
};

use crossterm::event::{Event, MouseButton, MouseEventKind};
use diffo_core::{AccessMode, ChangeKind, FileState, RepositorySnapshot};
use diffo_diff::{
    DiffDocument, RenderLine, RowKind, SideBySideRow, inline_rows, parse_unified_patch,
    side_by_side_rows,
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

mod input;

pub use input::map_event;

pub struct Renderer {
    highlighter: Arc<SyntaxHighlighter>,
    highlighted: Option<HighlightCache>,
    prepare_tx: SyncSender<PrepareRequest>,
    prepare_rx: Receiver<PrepareOutcome>,
    pending: Option<(PathBuf, String)>,
    failed: Option<(PathBuf, String)>,
    scrollbars: ScrollbarMetrics,
    scrollbar_drag: Option<ScrollbarAxis>,
    content_revision: u64,
    network_animation_tick: usize,
    #[cfg(test)]
    highlight_computations: usize,
}

struct HighlightCache {
    path: PathBuf,
    patch: String,
    document: DiffDocument,
    inline: Vec<RenderLine>,
    side_by_side: Vec<SideBySideRow>,
    inline_width: usize,
    highlighted: HighlightedDiff,
    #[cfg(test)]
    syntax_highlighted: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FramePreparation {
    pub maximum_vertical_scroll: usize,
    pub maximum_horizontal_scroll: usize,
    pub content_revision: u64,
    pub preparing: bool,
    pub anchored_vertical_scroll: Option<usize>,
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

struct PrepareRequest {
    path: PathBuf,
    patch: String,
}

#[derive(Clone, Copy, Debug, Default)]
struct ScrollbarMetrics {
    vertical_area: Rect,
    horizontal_area: Rect,
    rows: usize,
    columns: usize,
    viewport_rows: usize,
    viewport_columns: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScrollbarAxis {
    Vertical,
    Horizontal,
}

const MAX_HIGHLIGHT_BYTES: usize = 256 * 1024;
const MAX_HIGHLIGHT_LINES: usize = 2_000;
const MAX_SYNC_BYTES: usize = 64 * 1024;
const MAX_SYNC_LINES: usize = 500;

type PrepareOutcome = Result<HighlightCache, (PathBuf, String)>;

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
        thread::Builder::new()
            .name("diffo-diff-prepare".to_owned())
            .spawn(move || {
                while let Ok(request) = requests.recv() {
                    let key = (request.path.clone(), request.patch.clone());
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
            pending: None,
            failed: None,
            scrollbars: ScrollbarMetrics::default(),
            scrollbar_drag: None,
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
        let viewport_rows = usize::from(diff_area.height.saturating_sub(2));
        let viewport_columns = usize::from(diff_area.width.saturating_sub(2));
        let current_diff = model.selected.as_ref().and_then(|selected| {
            let file = model
                .snapshot
                .files
                .iter()
                .find(|file| file.path == selected.path)?;
            let diff = match selected.area {
                ChangeArea::Unstaged => file.unstaged.as_ref(),
                ChangeArea::Staged => file.staged.as_ref(),
            }?;
            Some((file.path.as_path(), diff.text.as_str()))
        });
        let previous_revision = self.content_revision;
        let anchor = current_diff.and_then(|(path, _)| {
            self.highlighted
                .as_ref()
                .filter(|cache| cache.path == path)
                .map(|cache| ScrollAnchor::capture(cache, model.diff_view_mode, model.diff_scroll))
        });
        if let Some((path, patch)) = current_diff {
            self.prepared_diff(path, patch);
        }
        let anchored_vertical_scroll = (self.content_revision != previous_revision)
            .then(|| {
                anchor.and_then(|anchor| {
                    self.highlighted
                        .as_ref()
                        .and_then(|cache| anchor.resolve(cache, model.diff_view_mode))
                })
            })
            .flatten();
        let failed_rows = current_diff.and_then(|(path, patch)| {
            self.failed
                .as_ref()
                .is_some_and(|failed| failed.0 == path && failed.1 == patch)
                .then(|| patch.lines().count())
        });
        let (rows, columns) = failed_rows.map_or_else(
            || {
                self.highlighted
                    .as_ref()
                    .map_or((0, 0), |cache| match model.diff_view_mode {
                        DiffViewMode::Inline => (cache.inline.len(), cache.inline_width),
                        DiffViewMode::SideBySide => (cache.side_by_side.len(), viewport_columns),
                    })
            },
            |rows| (rows, viewport_columns),
        );
        FramePreparation {
            maximum_vertical_scroll: rows.saturating_sub(viewport_rows),
            maximum_horizontal_scroll: columns.saturating_sub(viewport_columns),
            content_revision: self.content_revision,
            preparing: self.pending.is_some(),
            anchored_vertical_scroll,
        }
    }

    #[must_use]
    pub fn is_preparing(&self) -> bool {
        self.pending.is_some()
    }

    pub fn map_event(
        &mut self,
        event: &Event,
        model: &Model,
        area: Rect,
    ) -> Option<diffo_app::Message> {
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
            if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
                self.scrollbar_drag = None;
            } else if matches!(
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
                    return Some(self.scrollbar_message(axis, mouse.column, mouse.row));
                }
            }
        }
        input::map_event(event, model, area)
    }

    fn scrollbar_at(&self, column: u16, row: u16) -> Option<ScrollbarAxis> {
        if self.scrollbars.rows > self.scrollbars.viewport_rows
            && self.scrollbars.vertical_area.contains((column, row).into())
        {
            Some(ScrollbarAxis::Vertical)
        } else if self.scrollbars.columns > self.scrollbars.viewport_columns
            && self
                .scrollbars
                .horizontal_area
                .contains((column, row).into())
        {
            Some(ScrollbarAxis::Horizontal)
        } else {
            None
        }
    }

    fn scrollbar_message(&self, axis: ScrollbarAxis, column: u16, row: u16) -> diffo_app::Message {
        match axis {
            ScrollbarAxis::Vertical => diffo_app::Message::SetDiffScroll(scrollbar_position(
                row.saturating_sub(self.scrollbars.vertical_area.y),
                self.scrollbars.vertical_area.height,
                self.scrollbars.rows,
                self.scrollbars.viewport_rows,
            )),
            ScrollbarAxis::Horizontal => {
                diffo_app::Message::SetDiffHorizontalScroll(scrollbar_position(
                    column.saturating_sub(self.scrollbars.horizontal_area.x),
                    self.scrollbars.horizontal_area.width,
                    self.scrollbars.columns,
                    self.scrollbars.viewport_columns,
                ))
            }
        }
    }

    fn prepared_diff(&mut self, path: &Path, source_patch: &str) -> Option<&HighlightCache> {
        while let Ok(outcome) = self.prepare_rx.try_recv() {
            let outcome_key = match &outcome {
                Ok(cache) => (&cache.path, &cache.patch),
                Err(key) => (&key.0, &key.1),
            };
            if self
                .pending
                .as_ref()
                .is_some_and(|pending| pending.0 == *outcome_key.0 && pending.1 == *outcome_key.1)
            {
                self.pending = None;
            }
            match outcome {
                Ok(cache) => {
                    #[cfg(test)]
                    if cache.syntax_highlighted {
                        self.highlight_computations += 1;
                    }
                    self.failed = None;
                    self.install_cache(cache);
                }
                Err(key) => self.failed = Some(key),
            }
        }
        if self
            .highlighted
            .as_ref()
            .is_some_and(|cache| cache.path == path && cache.patch == source_patch)
        {
            return self.highlighted.as_ref();
        }
        let request_key = (path.to_path_buf(), source_patch.to_owned());
        if self.failed.as_ref() == Some(&request_key) {
            return None;
        }
        if source_patch.len() <= MAX_SYNC_BYTES && source_patch.lines().count() <= MAX_SYNC_LINES {
            let request = PrepareRequest {
                path: request_key.0.clone(),
                patch: request_key.1.clone(),
            };
            if let Some(cache) = prepare_diff(request, &self.highlighter) {
                #[cfg(test)]
                if cache.syntax_highlighted {
                    self.highlight_computations += 1;
                }
                self.install_cache(cache);
                return self.highlighted.as_ref();
            }
            self.failed = Some(request_key);
            return None;
        }
        if self.pending.as_ref() != Some(&request_key) {
            let request = PrepareRequest {
                path: request_key.0.clone(),
                patch: request_key.1.clone(),
            };
            match self.prepare_tx.try_send(request) {
                Ok(()) => self.pending = Some(request_key),
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {}
            }
        }
        None
    }

    fn install_cache(&mut self, cache: HighlightCache) {
        let changed = self
            .highlighted
            .as_ref()
            .is_none_or(|current| current.path != cache.path || current.patch != cache.patch);
        self.highlighted = Some(cache);
        if changed {
            self.content_revision = self.content_revision.saturating_add(1);
        }
    }

    fn render_diff(&mut self, frame: &mut Frame, area: ratatui::layout::Rect, model: &Model) {
        let mode = match model.diff_view_mode {
            DiffViewMode::Inline => "Inline",
            DiffViewMode::SideBySide => "Side by side",
        };
        let lines = self.diff_lines(
            model,
            area.width.saturating_sub(2),
            model.diff_scroll,
            usize::from(area.height.saturating_sub(2)),
        );
        let resize_label = if model.resizing_file_pane {
            format!(" · files {}%", model.file_pane_percent)
        } else {
            String::new()
        };
        let pane = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(resize_border_style(model))
                    .title(format!(" File Diff · {mode}{resize_label} ")),
            )
            .scroll((
                0,
                model.diff_horizontal_scroll.try_into().unwrap_or(u16::MAX),
            ));
        frame.render_widget(pane, area);

        let viewport_rows = usize::from(area.height.saturating_sub(2));
        let viewport_columns = usize::from(area.width.saturating_sub(2));
        let (rows, columns) =
            self.highlighted
                .as_ref()
                .map_or((0, 0), |cache| match model.diff_view_mode {
                    DiffViewMode::Inline => (cache.inline.len(), cache.inline_width),
                    DiffViewMode::SideBySide => (cache.side_by_side.len(), viewport_columns),
                });
        self.scrollbars = ScrollbarMetrics {
            vertical_area: Rect::new(
                area.right().saturating_sub(2),
                area.y.saturating_add(1),
                u16::from(area.width > 2),
                // Leave the bottom-right corner to the horizontal scrollbar so
                // its final cell remains reachable with the mouse.
                area.height.saturating_sub(3),
            ),
            horizontal_area: Rect::new(
                area.x.saturating_add(1),
                area.bottom().saturating_sub(2),
                area.width.saturating_sub(2),
                u16::from(area.height > 2),
            ),
            rows,
            columns,
            viewport_rows,
            viewport_columns,
        };
        if rows > viewport_rows {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .thumb_style(Style::default().fg(Color::Cyan));
            let mut state = ScrollbarState::new(scrollbar_position_count(rows, viewport_rows))
                .viewport_content_length(viewport_rows)
                .position(model.diff_scroll);
            frame.render_stateful_widget(scrollbar, self.scrollbars.vertical_area, &mut state);
        }
        if columns > viewport_columns {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
                .begin_symbol(None)
                .end_symbol(None)
                .thumb_style(Style::default().fg(Color::Cyan));
            let mut state =
                ScrollbarState::new(scrollbar_position_count(columns, viewport_columns))
                    .viewport_content_length(viewport_columns)
                    .position(model.diff_horizontal_scroll);
            frame.render_stateful_widget(scrollbar, self.scrollbars.horizontal_area, &mut state);
        }
    }

    fn diff_lines(
        &mut self,
        model: &Model,
        width: u16,
        first_row: usize,
        row_count: usize,
    ) -> Vec<Line<'static>> {
        let Some(selected) = model.selected.as_ref() else {
            return vec![Line::raw("No file selected.")];
        };
        let Some(file) = model
            .snapshot
            .files
            .iter()
            .find(|file| file.path == selected.path)
        else {
            return vec![Line::raw("Selected file is no longer available.")];
        };
        let diff = match selected.area {
            ChangeArea::Unstaged => file.unstaged.as_ref(),
            ChangeArea::Staged => file.staged.as_ref(),
        };
        let Some(diff) = diff else {
            return vec![Line::raw("No text diff is available for this file.")];
        };
        let ready = self.prepared_diff(&file.path, &diff.text).is_some();
        if !ready
            && self
                .failed
                .as_ref()
                .is_some_and(|failed| failed.0 == file.path && failed.1 == diff.text)
        {
            return diff
                .text
                .lines()
                .skip(first_row)
                .take(row_count)
                .map(|line| Line::raw(line.to_owned()))
                .collect();
        }
        let Some(cache) = self.highlighted.as_ref() else {
            return Vec::new();
        };
        if cache.document.binary {
            return vec![Line::raw("Binary file changed.")];
        }

        match model.diff_view_mode {
            DiffViewMode::Inline => cache
                .inline
                .iter()
                .skip(first_row)
                .take(row_count)
                .map(|row| inline_line(row, &cache.highlighted, usize::from(width)))
                .collect(),
            DiffViewMode::SideBySide => {
                let column_width = usize::from(width.saturating_sub(3) / 2);
                cache
                    .side_by_side
                    .iter()
                    .skip(first_row)
                    .take(row_count)
                    .map(|row| side_by_side_line(row, column_width, &cache.highlighted))
                    .collect()
            }
        }
    }
}

impl ScrollAnchor {
    fn capture(cache: &HighlightCache, mode: DiffViewMode, first_row: usize) -> Self {
        let row_count = projection_len(cache, mode);
        Self {
            rows: (first_row..row_count)
                .take(16)
                .filter_map(|index| {
                    anchor_row(cache, mode, index).map(|row| (index - first_row, index, row))
                })
                .collect(),
        }
    }

    fn resolve(&self, cache: &HighlightCache, mode: DiffViewMode) -> Option<usize> {
        let row_count = projection_len(cache, mode);
        self.rows
            .iter()
            .find_map(|(viewport_offset, old_index, anchor)| {
                (0..row_count)
                    .filter(|index| anchor.matches(cache, mode, *index))
                    .min_by_key(|index| index.abs_diff(*old_index))
                    .map(|index| index.saturating_sub(*viewport_offset))
            })
    }
}

impl AnchorRow {
    fn matches(&self, cache: &HighlightCache, mode: DiffViewMode, index: usize) -> bool {
        match (self, mode) {
            (Self::Inline { kind, text }, DiffViewMode::Inline) => cache
                .inline
                .get(index)
                .is_some_and(|row| row.kind == *kind && row.text == *text),
            (Self::SideBySide { old, new }, DiffViewMode::SideBySide) => {
                cache.side_by_side.get(index).is_some_and(|row| {
                    side_line_matches(old.as_ref(), row.old.as_ref())
                        && side_line_matches(new.as_ref(), row.new.as_ref())
                })
            }
            _ => false,
        }
    }
}

fn side_line_matches(expected: Option<&(RowKind, String)>, actual: Option<&RenderLine>) -> bool {
    match (expected, actual) {
        (Some((kind, text)), Some(actual)) => actual.kind == *kind && actual.text == *text,
        (None, None) => true,
        _ => false,
    }
}

fn projection_len(cache: &HighlightCache, mode: DiffViewMode) -> usize {
    match mode {
        DiffViewMode::Inline => cache.inline.len(),
        DiffViewMode::SideBySide => cache.side_by_side.len(),
    }
}

fn anchor_row(cache: &HighlightCache, mode: DiffViewMode, index: usize) -> Option<AnchorRow> {
    match mode {
        DiffViewMode::Inline => cache.inline.get(index).map(|row| AnchorRow::Inline {
            kind: row.kind,
            text: row.text.clone(),
        }),
        DiffViewMode::SideBySide => {
            cache
                .side_by_side
                .get(index)
                .map(|row| AnchorRow::SideBySide {
                    old: row.old.as_ref().map(|line| (line.kind, line.text.clone())),
                    new: row.new.as_ref().map(|line| (line.kind, line.text.clone())),
                })
        }
    }
}

fn render_command_palette(frame: &mut Frame, model: &Model) {
    let Some(palette) = model.command_palette.as_ref() else {
        return;
    };
    let commands = palette.matches();
    let (area, results_area) = command_palette_layout(frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Command Palette "),
        area,
    );
    let inner = area.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 2,
    });
    let sections = command_palette_sections(inner);
    frame.render_widget(
        Paragraph::new(format!("> {}█", palette.query)).style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        sections[0],
    );
    frame.render_widget(
        Paragraph::new("─".repeat(usize::from(sections[1].width)))
            .style(Style::default().fg(Color::DarkGray)),
        sections[1],
    );
    let items = if commands.is_empty() {
        vec![ListItem::new("No matching commands").style(Style::default().fg(Color::DarkGray))]
    } else {
        commands
            .iter()
            .map(|command| ListItem::new(command.label))
            .collect()
    };
    let list = List::new(items).highlight_symbol("› ").highlight_style(
        Style::default()
            .bg(Color::Indexed(24))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default().with_selected(
        (!commands.is_empty()).then_some(palette.selected.min(commands.len().saturating_sub(1))),
    );
    frame.render_stateful_widget(list, results_area, &mut state);
    frame.render_widget(
        Paragraph::new("type to search · ↑↓ select · enter run · esc close")
            .style(Style::default().fg(Color::DarkGray)),
        sections[3],
    );
}

fn render_help(frame: &mut Frame, model: &Model) {
    if !model.help_open {
        return;
    }
    let area = help_layout(frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Help ");
    let inner = block.inner(area).inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 2,
    });
    frame.render_widget(block, area);
    let sections = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
    let rows = input::help_rows(model.access_mode)
        .into_iter()
        .map(|(keys, description)| {
            Row::new([
                Cell::from(keys).style(
                    Style::default()
                        .fg(Color::LightCyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Cell::from(description).style(Style::default().fg(Color::White)),
            ])
        });
    let table = Table::new(rows, [Constraint::Length(22), Constraint::Min(24)])
        .header(
            Row::new(["Shortcut", "Action"])
                .style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
                .bottom_margin(1),
        )
        .column_spacing(2);
    frame.render_widget(table, sections[0]);
    let footer = if model.access_mode == AccessMode::ReadOnly {
        "Esc: close  ·  Read-only: repository actions are disabled"
    } else {
        "Esc: close"
    };
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(Color::DarkGray)),
        sections[1],
    );
}

fn render_toasts(frame: &mut Frame, model: &Model) {
    for (toast, area) in model.toasts.iter().zip(toast_areas(model, frame.area())) {
        let color = match toast.kind {
            ToastKind::Success => Color::LightGreen,
            ToastKind::Info => Color::LightCyan,
            ToastKind::Error => Color::LightRed,
        };
        frame.render_widget(Clear, area);
        let text = toast.detail.as_ref().map_or_else(
            || toast.title.clone(),
            |detail| format!("{}\n{detail}", toast.title),
        );
        frame.render_widget(
            Paragraph::new(text)
                .wrap(ratatui::widgets::Wrap { trim: true })
                .style(Style::default().fg(Color::White))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(color)),
                ),
            area,
        );
    }
}

fn toast_areas(model: &Model, area: Rect) -> Vec<Rect> {
    let width = 44.min(area.width.saturating_sub(2));
    let right = area.right().saturating_sub(1);
    let mut bottom = area.bottom().saturating_sub(2);
    model
        .toasts
        .iter()
        .filter_map(|toast| {
            let inner_width = usize::from(width.saturating_sub(2)).max(1);
            let text_rows = std::iter::once(toast.title.as_str())
                .chain(toast.detail.as_deref())
                .map(|text| text.chars().count().div_ceil(inner_width))
                .sum::<usize>();
            let height = u16::try_from(text_rows)
                .unwrap_or(u16::MAX)
                .saturating_add(2)
                .clamp(3, 6);
            if width < 4 || bottom < area.y.saturating_add(height) {
                return None;
            }
            let rect = Rect::new(right.saturating_sub(width), bottom - height, width, height);
            bottom = rect.y;
            Some(rect)
        })
        .collect()
}

fn toast_at_position(model: &Model, area: Rect, column: u16, row: u16) -> Option<u64> {
    model
        .toasts
        .iter()
        .zip(toast_areas(model, area))
        .find_map(|(toast, area)| area.contains((column, row).into()).then_some(toast.id))
}

fn render_commit_editor(frame: &mut Frame, model: &Model) {
    if !model.commit_input_focused() {
        return;
    }
    let (area, input, commit, cancel, footer) = commit_editor_layout(frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Commit message "),
        area,
    );

    let empty = model.commit_message.is_empty();
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::LightCyan));
    let input_inner = input_block.inner(input);
    let field_width = usize::from(input_inner.width);
    let cursor = model.commit_message_cursor();
    let (message, cursor_offset) = if empty {
        (
            model
                .suggested_commit_message()
                .unwrap_or_else(|| "Type a message…".to_owned()),
            0,
        )
    } else {
        let start = cursor.saturating_sub(field_width.saturating_sub(1));
        (
            model
                .commit_message
                .chars()
                .skip(start)
                .take(field_width)
                .collect(),
            cursor.saturating_sub(start),
        )
    };
    frame.render_widget(
        Paragraph::new(message).style(if empty {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        }),
        input_inner,
    );
    frame.render_widget(input_block, input);

    let commit_style = if model.primary_action() == diffo_app::PrimaryAction::Commit
        && model.primary_action_enabled()
    {
        Style::default()
            .bg(Color::Indexed(24))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    frame.render_widget(
        Paragraph::new("[ Commit ]")
            .alignment(Alignment::Center)
            .style(commit_style),
        commit,
    );
    frame.render_widget(
        Paragraph::new("[ Cancel ]")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::White)),
        cancel,
    );
    frame.render_widget(
        Paragraph::new("Enter: commit · Esc: cancel · click outside: close")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        footer,
    );

    let cursor_offset = cursor_offset.min(usize::from(input_inner.width.saturating_sub(1)));
    let cursor_x = input_inner
        .x
        .saturating_add(u16::try_from(cursor_offset).unwrap_or(u16::MAX));
    frame.set_cursor_position((cursor_x, input_inner.y));
}

fn commit_editor_layout(area: Rect) -> (Rect, Rect, Rect, Rect, Rect) {
    let width = (area.width.saturating_mul(7) / 10).clamp(34.min(area.width), 84.min(area.width));
    let height = 11.min(area.height);
    let modal = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let inner = modal.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 2,
    });
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);
    let buttons =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[2]);
    (modal, rows[0], buttons[0], buttons[1], rows[4])
}

pub(crate) fn commit_editor_action_at_position(
    model: &Model,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<diffo_app::Message> {
    let (dialog_area, _input, commit, cancel, _footer) = commit_editor_layout(area);
    let position = (column, row).into();
    if !dialog_area.contains(position) {
        return Some(diffo_app::Message::BlurCommitInput);
    }
    if cancel.contains(position) {
        return Some(diffo_app::Message::BlurCommitInput);
    }
    if commit.contains(position)
        && model.primary_action() == diffo_app::PrimaryAction::Commit
        && model.primary_action_enabled()
    {
        return Some(diffo_app::Message::ExecutePrimaryAction);
    }
    None
}

fn help_layout(area: Rect) -> Rect {
    let width = (area.width.saturating_mul(4) / 5).clamp(40.min(area.width), 90.min(area.width));
    let top = area.y.saturating_add(area.height.saturating_mul(10) / 100);
    let height = 24.min(area.bottom().saturating_sub(top));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        top,
        width,
        height,
    )
}

fn command_palette_layout(area: Rect) -> (Rect, Rect) {
    let width = (area.width.saturating_mul(7) / 10).clamp(30.min(area.width), 80.min(area.width));
    let top = area.y.saturating_add(area.height.saturating_mul(20) / 100);
    let height = 18.min(area.bottom().saturating_sub(top));
    let palette = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        top,
        width,
        height,
    );
    let inner = palette.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 2,
    });
    let sections = command_palette_sections(inner);
    (palette, sections[2])
}

fn command_palette_sections(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area)
}

fn prepare_diff(
    request: PrepareRequest,
    highlighter: &SyntaxHighlighter,
) -> Option<HighlightCache> {
    let document = parse_unified_patch(&request.patch).ok()?;
    let syntax_highlighted = request.patch.len() <= MAX_HIGHLIGHT_BYTES
        && request.patch.lines().count() <= MAX_HIGHLIGHT_LINES;
    let syntax_styles = if syntax_highlighted {
        highlighter.highlight(&request.path, &document)
    } else {
        HighlightedDiff::default()
    };
    let inline = inline_rows(&document);
    let inline_width = inline
        .iter()
        .map(|row| row.text.chars().count().saturating_add(7))
        .max()
        .unwrap_or(0);
    let side_by_side = side_by_side_rows(&document);
    Some(HighlightCache {
        path: request.path,
        patch: request.patch,
        document,
        inline,
        side_by_side,
        inline_width,
        highlighted: syntax_styles,
        #[cfg(test)]
        syntax_highlighted,
    })
}

fn scrollbar_position(
    coordinate: u16,
    track_length: u16,
    content_length: usize,
    viewport_length: usize,
) -> usize {
    let maximum = content_length.saturating_sub(viewport_length);
    if track_length <= 1 {
        return 0;
    }
    usize::from(coordinate.min(track_length - 1)) * maximum / usize::from(track_length - 1)
}

fn scrollbar_position_count(content_length: usize, viewport_length: usize) -> usize {
    content_length
        .saturating_sub(viewport_length)
        .saturating_add(1)
}

pub(crate) fn file_at_position(
    model: &Model,
    area: ratatui::layout::Rect,
    column: u16,
    row: u16,
) -> Option<FileKey> {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);
    let columns = horizontal_panes(vertical[0], model.file_pane_percent);
    let file_areas = file_panel_areas(columns[0]);
    let groups = file_group_areas(file_areas[1]);
    file_in_group_at(
        staged_files(&model.snapshot),
        ChangeArea::Staged,
        groups[0],
        column,
        row,
    )
    .or_else(|| {
        file_in_group_at(
            unstaged_files(&model.snapshot),
            ChangeArea::Unstaged,
            groups[1],
            column,
            row,
        )
    })
}

pub(crate) fn file_action_at_position(
    model: &Model,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<diffo_app::Message> {
    if model.access_mode == AccessMode::ReadOnly {
        return None;
    }
    let columns = horizontal_panes(main_area(area), model.file_pane_percent);
    let file_areas = file_panel_areas(columns[0]);
    let groups = file_group_areas(file_areas[1]);
    if header_action_contains(groups[0], " Staged [", column, row) {
        return Some(diffo_app::Message::UnstageAll);
    }
    if header_action_contains(groups[1], " Changes [", column, row) {
        return Some(diffo_app::Message::StageAll);
    }
    for (group, change_area) in [
        (groups[0], ChangeArea::Staged),
        (groups[1], ChangeArea::Unstaged),
    ] {
        let button_start = group.right().saturating_sub(4);
        if column < button_start || column >= group.right().saturating_sub(1) {
            continue;
        }
        let key = match change_area {
            ChangeArea::Staged => file_in_group_at(
                staged_files(&model.snapshot),
                change_area,
                group,
                column,
                row,
            ),
            ChangeArea::Unstaged => file_in_group_at(
                unstaged_files(&model.snapshot),
                change_area,
                group,
                column,
                row,
            ),
        };
        let Some(key) = key else {
            continue;
        };
        return Some(match change_area {
            ChangeArea::Staged => diffo_app::Message::UnstageFile(key.path),
            ChangeArea::Unstaged => diffo_app::Message::StageFile(key.path),
        });
    }
    None
}

fn header_action_contains(area: Rect, prefix: &str, column: u16, row: u16) -> bool {
    let button = area
        .x
        .saturating_add(1)
        .saturating_add(u16::try_from(prefix.chars().count()).unwrap_or(u16::MAX));
    row == area.y && column == button && button < area.right().saturating_sub(1)
}

pub(crate) fn is_file_pane_splitter_at(
    model: &Model,
    area: ratatui::layout::Rect,
    column: u16,
    row: u16,
) -> bool {
    let main = main_area(area);
    if row < main.y || row >= main.y.saturating_add(main.height) {
        return false;
    }
    let panes = horizontal_panes(main, model.file_pane_percent);
    let splitter = panes[1].x;
    column.abs_diff(splitter) <= 1
}

pub(crate) fn file_pane_percent_at(area: ratatui::layout::Rect, column: u16) -> u16 {
    let main = main_area(area);
    if main.width == 0 {
        return 0;
    }
    let offset = column.saturating_sub(main.x).min(main.width);
    u16::try_from(u32::from(offset) * 100 / u32::from(main.width)).unwrap_or(100)
}

fn main_area(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area)[0]
}

fn horizontal_panes(
    area: ratatui::layout::Rect,
    file_pane_percent: u16,
) -> std::rc::Rc<[ratatui::layout::Rect]> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(file_pane_percent.min(100)),
            Constraint::Percentage(100_u16.saturating_sub(file_pane_percent)),
        ])
        .split(area)
}

fn file_in_group_at<'a>(
    mut files: impl Iterator<Item = &'a FileState>,
    change_area: ChangeArea,
    area: ratatui::layout::Rect,
    column: u16,
    row: u16,
) -> Option<FileKey> {
    let inside = column > area.x
        && column < area.x.saturating_add(area.width).saturating_sub(1)
        && row > area.y
        && row < area.y.saturating_add(area.height).saturating_sub(1);
    if !inside {
        return None;
    }
    files
        .nth(usize::from(row - area.y - 1))
        .map(|file| FileKey {
            path: file.path.clone(),
            area: change_area,
        })
}

fn render_files(frame: &mut Frame, area: ratatui::layout::Rect, model: &Model) {
    let panels = file_panel_areas(area);
    render_commit_composer(frame, panels[0], model);
    let groups = file_group_areas(panels[1]);
    render_file_group(
        frame,
        groups[0],
        if model.access_mode == AccessMode::ReadOnly {
            " Staged Changes "
        } else {
            " Staged [-] Unstage All "
        },
        staged_files(&model.snapshot),
        ChangeArea::Staged,
        model,
    );
    let changes_title = if model.access_mode == AccessMode::ReadOnly {
        " Changes · read-only "
    } else {
        " Changes [+] Stage All "
    };
    render_file_group(
        frame,
        groups[1],
        changes_title,
        unstaged_files(&model.snapshot),
        ChangeArea::Unstaged,
        model,
    );
}

fn file_panel_areas(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::vertical([Constraint::Length(6), Constraint::Min(2)]).split(area)
}

fn commit_composer_areas(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .split(area)
}

fn render_commit_composer(frame: &mut Frame, area: Rect, model: &Model) {
    let sections = commit_composer_areas(area);
    let empty = model.commit_message.is_empty();
    let message = if empty {
        model
            .suggested_commit_message()
            .unwrap_or_else(|| "Type a message…".to_owned())
    } else {
        model.commit_message.clone()
    };
    frame.render_widget(
        Paragraph::new(message)
            .style(if empty {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Commit message · click to edit "),
            ),
        sections[0],
    );
    let action = model.primary_action();
    let style = if model.primary_action_enabled() {
        Style::default()
            .bg(Color::Indexed(24))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    frame.render_widget(
        Paragraph::new(format!("[ {} ]", action.label()))
            .alignment(Alignment::Center)
            .style(style),
        sections[1],
    );
}

pub(crate) fn commit_action_at_position(
    model: &Model,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<diffo_app::Message> {
    let columns = horizontal_panes(main_area(area), model.file_pane_percent);
    let file_areas = file_panel_areas(columns[0]);
    let sections = commit_composer_areas(file_areas[0]);
    if sections[0].contains((column, row).into()) {
        return Some(diffo_app::Message::FocusCommitInput);
    }
    if sections[1].contains((column, row).into())
        && (model.primary_action_enabled()
            || model.primary_action() == diffo_app::PrimaryAction::PushAndPull)
    {
        return Some(diffo_app::Message::ExecutePrimaryAction);
    }
    None
}

fn file_group_areas(area: ratatui::layout::Rect) -> std::rc::Rc<[ratatui::layout::Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area)
}

fn render_file_group<'a>(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    title: &str,
    files: impl Iterator<Item = &'a FileState>,
    change_area: ChangeArea,
    model: &Model,
) {
    let files = files.collect::<Vec<_>>();
    let selected = files
        .iter()
        .position(|file| model.is_selected(&file.path, change_area));
    let items = files.into_iter().map(|file| {
        file_item(
            file,
            model.is_selected(&file.path, change_area),
            change_area,
            usize::from(area.width.saturating_sub(4)),
            model.access_mode,
        )
    });
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(resize_border_style(model))
                .title(title),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ")
        .highlight_spacing(HighlightSpacing::Always);
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, area, &mut state);
}

fn file_item(
    file: &FileState,
    selected: bool,
    change_area: ChangeArea,
    width: usize,
    access_mode: AccessMode,
) -> ListItem<'static> {
    let marker = match file.kind {
        ChangeKind::Added | ChangeKind::Untracked => "A",
        ChangeKind::Modified => "M",
        ChangeKind::Deleted => "D",
        ChangeKind::Renamed => "R",
        ChangeKind::Copied => "C",
        ChangeKind::Conflicted => "U",
    };
    let label = format!("{marker}  {}", file.path.display());
    let style = file_kind_style(file.kind, selected);
    if access_mode == AccessMode::ReadOnly || width < 3 {
        return ListItem::new(Line::styled(label, style));
    }
    let action = match change_area {
        ChangeArea::Staged => "[-]",
        ChangeArea::Unstaged => "[+]",
    };
    let label_width = width.saturating_sub(action.len());
    let mut label = label.chars().take(label_width).collect::<String>();
    label.push_str(&" ".repeat(label_width.saturating_sub(label.chars().count())));
    ListItem::new(Line::from(vec![
        Span::styled(label, style),
        Span::styled(action, file_action_style(change_area)),
    ]))
}

fn file_kind_style(kind: ChangeKind, selected: bool) -> Style {
    let style = match kind {
        ChangeKind::Added | ChangeKind::Untracked => Style::default().fg(Color::LightGreen),
        ChangeKind::Modified => Style::default().fg(Color::Yellow),
        ChangeKind::Deleted => Style::default()
            .fg(Color::LightRed)
            .add_modifier(Modifier::CROSSED_OUT),
        ChangeKind::Renamed | ChangeKind::Copied => Style::default().fg(Color::LightCyan),
        ChangeKind::Conflicted => Style::default()
            .fg(Color::LightRed)
            .add_modifier(Modifier::BOLD),
    };
    if selected {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

fn file_action_style(change_area: ChangeArea) -> Style {
    let color = match change_area {
        ChangeArea::Staged => Color::LightRed,
        ChangeArea::Unstaged => Color::LightGreen,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn inline_line(row: &RenderLine, highlighted: &HighlightedDiff, width: usize) -> Line<'static> {
    let prefix = match row.kind {
        RowKind::Removed => "-",
        RowKind::Added => "+",
        RowKind::Conflict => "!",
        RowKind::Header => "@",
        _ => " ",
    };
    let number = row
        .number
        .map_or_else(|| "    ".to_owned(), |number| format!("{number:>4}"));
    if matches!(row.kind, RowKind::Header | RowKind::Meta) {
        return Line::styled(
            format!("{number} {prefix} {}", row.text),
            row_style(row.kind),
        );
    }
    let mut spans = vec![Span::styled(
        format!("{number} {prefix} "),
        gutter_style(row.kind),
    )];
    spans.extend(code_spans(row, highlighted));
    pad_to_width(&mut spans, width, diff_background(row.kind));
    Line::from(spans)
}

fn side_by_side_line(
    row: &SideBySideRow,
    column_width: usize,
    highlighted: &HighlightedDiff,
) -> Line<'static> {
    let mut spans = format_cell(row.old.as_ref(), column_width, highlighted);
    spans.push(Span::raw(" │ "));
    spans.extend(format_cell(row.new.as_ref(), column_width, highlighted));
    Line::from(spans)
}

fn format_cell(
    line: Option<&RenderLine>,
    width: usize,
    highlighted: &HighlightedDiff,
) -> Vec<Span<'static>> {
    let Some(line) = line else {
        return vec![Span::raw(" ".repeat(width))];
    };
    let number = line
        .number
        .map_or_else(|| "    ".to_owned(), |number| format!("{number:>4}"));
    if matches!(line.kind, RowKind::Header | RowKind::Meta) {
        return clip_and_pad(
            vec![Span::styled(
                format!("{number} {}", line.text),
                row_style(line.kind),
            )],
            width,
            Style::default(),
        );
    }
    let mut spans = vec![Span::styled(format!("{number} "), gutter_style(line.kind))];
    spans.extend(code_spans(line, highlighted));
    clip_and_pad(spans, width, diff_background(line.kind))
}

fn code_spans(row: &RenderLine, highlighted: &HighlightedDiff) -> Vec<Span<'static>> {
    let highlighted_line = row.number.and_then(|number| match row.kind {
        RowKind::Removed => highlighted.old.get(&number),
        RowKind::Added | RowKind::Context | RowKind::Changed => highlighted.new.get(&number),
        RowKind::Header | RowKind::Conflict | RowKind::Meta => None,
    });
    let background = diff_background(row.kind);
    highlighted_line.map_or_else(
        || vec![Span::styled(row.text.clone(), background)],
        |line| syntax_spans(line, background, row.kind),
    )
}

fn syntax_spans(
    line: &HighlightedLine,
    background: Style,
    row_kind: RowKind,
) -> Vec<Span<'static>> {
    line.spans
        .iter()
        .map(|span| {
            Span::styled(
                span.text.clone(),
                syntax_style(span, row_kind).patch(background),
            )
        })
        .collect()
}

fn syntax_style(span: &StyledSpan, row_kind: RowKind) -> Style {
    let foreground = contrasting_foreground(span.foreground, row_kind);
    Style::default().fg(Color::Rgb(
        foreground.red,
        foreground.green,
        foreground.blue,
    ))
}

fn contrasting_foreground(foreground: Rgb, row_kind: RowKind) -> Rgb {
    let Some(background) = diff_background_rgb(row_kind) else {
        return foreground;
    };
    if contrast_ratio(foreground, background) >= 4.5 {
        return foreground;
    }
    for step in 1..=10 {
        let candidate = Rgb {
            red: lighten_channel(foreground.red, step),
            green: lighten_channel(foreground.green, step),
            blue: lighten_channel(foreground.blue, step),
        };
        if contrast_ratio(candidate, background) >= 4.5 {
            return candidate;
        }
    }
    Rgb {
        red: u8::MAX,
        green: u8::MAX,
        blue: u8::MAX,
    }
}

fn lighten_channel(channel: u8, step: u16) -> u8 {
    let channel = u16::from(channel);
    let lightened = channel + (u16::from(u8::MAX) - channel) * step / 10;
    u8::try_from(lightened).expect("lightened color channel remains within u8")
}

fn contrast_ratio(foreground: Rgb, background: Rgb) -> f64 {
    let foreground = relative_luminance(foreground);
    let background = relative_luminance(background);
    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
}

fn relative_luminance(color: Rgb) -> f64 {
    0.2126 * linear_channel(color.red)
        + 0.7152 * linear_channel(color.green)
        + 0.0722 * linear_channel(color.blue)
}

fn linear_channel(channel: u8) -> f64 {
    let channel = f64::from(channel) / 255.0;
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn clip_and_pad(
    spans: Vec<Span<'static>>,
    width: usize,
    padding_style: Style,
) -> Vec<Span<'static>> {
    let mut remaining = width;
    let mut clipped = Vec::new();
    for span in spans {
        if remaining == 0 {
            break;
        }
        let text = span.content.chars().take(remaining).collect::<String>();
        remaining = remaining.saturating_sub(text.chars().count());
        clipped.push(Span::styled(text, span.style));
    }
    if remaining > 0 {
        clipped.push(Span::styled(" ".repeat(remaining), padding_style));
    }
    clipped
}

fn pad_to_width(spans: &mut Vec<Span<'static>>, width: usize, padding_style: Style) {
    let used = spans
        .iter()
        .map(|span| span.content.chars().count())
        .sum::<usize>();
    if used < width {
        spans.push(Span::styled(" ".repeat(width - used), padding_style));
    }
}

fn gutter_style(kind: RowKind) -> Style {
    let foreground = match kind {
        RowKind::Removed => Color::LightRed,
        RowKind::Added => Color::LightGreen,
        RowKind::Conflict => Color::LightYellow,
        RowKind::Header | RowKind::Context | RowKind::Changed | RowKind::Meta => Color::DarkGray,
    };
    Style::default().fg(foreground).patch(diff_background(kind))
}

fn diff_background(kind: RowKind) -> Style {
    match kind {
        // Use xterm-256 colors here instead of RGB. These survive SSH and terminal
        // multiplexers that advertise `xterm-256color` but filter true-color backgrounds.
        RowKind::Removed => Style::default().bg(Color::Indexed(52)),
        RowKind::Added => Style::default().bg(Color::Indexed(22)),
        RowKind::Conflict => Style::default().bg(Color::Indexed(58)),
        RowKind::Header | RowKind::Context | RowKind::Changed | RowKind::Meta => Style::default(),
    }
}

fn diff_background_rgb(kind: RowKind) -> Option<Rgb> {
    match kind {
        RowKind::Removed => Some(Rgb {
            red: 95,
            green: 0,
            blue: 0,
        }),
        RowKind::Added => Some(Rgb {
            red: 0,
            green: 95,
            blue: 0,
        }),
        RowKind::Conflict => Some(Rgb {
            red: 95,
            green: 95,
            blue: 0,
        }),
        RowKind::Header | RowKind::Context | RowKind::Changed | RowKind::Meta => None,
    }
}

fn row_style(kind: RowKind) -> Style {
    match kind {
        RowKind::Header => Style::default().fg(Color::Cyan),
        RowKind::Removed => Style::default().fg(Color::Red),
        RowKind::Added => Style::default().fg(Color::Green),
        RowKind::Conflict => Style::default()
            .fg(Color::LightYellow)
            .bg(Color::Indexed(58))
            .add_modifier(Modifier::BOLD),
        RowKind::Meta => Style::default().fg(Color::Yellow),
        RowKind::Context | RowKind::Changed => Style::default(),
    }
}

fn network_animation_style(tick: usize) -> Style {
    const GRADIENT: [u8; 12] = [24, 25, 31, 37, 43, 42, 36, 30, 24, 60, 54, 53];
    Style::default()
        .fg(Color::Indexed(GRADIENT[(tick / 4) % GRADIENT.len()]))
        .add_modifier(Modifier::BOLD)
}

fn render_status(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    model: &Model,
    animation_tick: usize,
) {
    let text = if let Some(error) = model.error.as_deref() {
        error.to_owned()
    } else if let Some(operation) = model.network_operation() {
        const SPINNER: [&str; 4] = ["◐", "◓", "◑", "◒"];
        format!(
            " {} {}… · Ctrl+C to exit ",
            SPINNER[(animation_tick / 2) % SPINNER.len()],
            operation.label()
        )
    } else if model.resizing_file_pane {
        format!(
            " Resizing file pane: {}% · release mouse to finish ",
            model.file_pane_percent
        )
    } else {
        " 1/f1: commands  2/f2: help ".to_owned()
    };
    let style = if model.error.is_some() {
        Style::default().fg(Color::Red)
    } else if model.network_operation().is_some() {
        network_animation_style(animation_tick)
    } else if model.resizing_file_pane {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}

fn resize_border_style(model: &Model) -> Style {
    if model.resizing_file_pane {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

fn unstaged_files(snapshot: &RepositorySnapshot) -> impl Iterator<Item = &FileState> {
    snapshot
        .files
        .iter()
        .filter(|file| file.unstaged.is_some() || file.kind == ChangeKind::Untracked)
}

fn staged_files(snapshot: &RepositorySnapshot) -> impl Iterator<Item = &FileState> {
    snapshot.files.iter().filter(|file| file.staged.is_some())
}

#[cfg(test)]
mod rendering_tests {
    use std::fmt::Write;
    use std::path::PathBuf;
    use std::thread::sleep;
    use std::time::Duration;
    use std::time::Instant;

    use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use diffo_app::{DiffViewMode, Model};
    use diffo_core::{
        AccessMode, ChangeKind, FileDiff, FileState, OperationResult, RepositoryAction,
        RepositorySnapshot, UpstreamState,
    };
    use diffo_diff::RowKind;
    use diffo_highlight::Rgb;
    use ratatui::{
        Terminal,
        backend::TestBackend,
        layout::Rect,
        style::{Color, Modifier},
    };

    use super::{
        Renderer, command_palette_layout, contrast_ratio, contrasting_foreground, diff_background,
        diff_background_rgb, file_kind_style, row_style, scrollbar_position_count,
    };

    #[test]
    fn file_list_styles_show_git_change_kinds() {
        assert_eq!(
            file_kind_style(ChangeKind::Untracked, false).fg,
            Some(Color::LightGreen)
        );
        assert_eq!(
            file_kind_style(ChangeKind::Added, false).fg,
            Some(Color::LightGreen)
        );
        assert_eq!(
            file_kind_style(ChangeKind::Modified, false).fg,
            Some(Color::Yellow)
        );
        let deleted = file_kind_style(ChangeKind::Deleted, false);
        assert_eq!(deleted.fg, Some(Color::LightRed));
        assert!(deleted.add_modifier.contains(Modifier::CROSSED_OUT));
        let conflicted = file_kind_style(ChangeKind::Conflicted, false);
        assert_eq!(conflicted.fg, Some(Color::LightRed));
        assert!(conflicted.add_modifier.contains(Modifier::BOLD));
        assert!(
            file_kind_style(ChangeKind::Added, true)
                .add_modifier
                .contains(Modifier::BOLD)
        );
    }

    #[test]
    fn scrollbar_length_is_the_number_of_legal_viewport_positions() {
        assert_eq!(scrollbar_position_count(120, 25), 96);
        assert_eq!(scrollbar_position_count(25, 25), 1);
    }

    #[test]
    fn conflict_markers_have_a_dedicated_high_contrast_style() {
        let marker = row_style(RowKind::Conflict);
        assert_eq!(marker.fg, Some(Color::LightYellow));
        assert_eq!(marker.bg, Some(Color::Indexed(58)));
        assert!(marker.add_modifier.contains(Modifier::BOLD));
        assert_eq!(diff_background(RowKind::Conflict).bg, marker.bg);
    }

    fn model() -> Model {
        Model::new(
            RepositorySnapshot {
                files: vec![FileState {
                    path: PathBuf::from("src/main.rs"),
                    old_path: None,
                    kind: ChangeKind::Modified,
                    staged: None,
                    unstaged: Some(FileDiff {
                        text: "@@ -1 +1 @@\n-let old = true;\n+let new = false;\n".to_owned(),
                    }),
                }],
                ..RepositorySnapshot::default()
            },
            AccessMode::ReadWrite,
        )
    }

    #[test]
    fn network_operations_animate_the_frame_and_name_the_operation() {
        let mut model = model();
        model.snapshot.files[0].unstaged = None;
        model.snapshot.upstream = Some(UpstreamState {
            name: "origin/main".to_owned(),
            ahead: 1,
            behind: 0,
        });
        assert_eq!(model.execute_primary_action(), Some(RepositoryAction::Push));

        let mut renderer = Renderer::new();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| renderer.render(frame, &model))
            .unwrap();
        let first_border = terminal.backend().buffer()[(0, 0)].fg;
        let screen =
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .fold(String::new(), |mut output, cell| {
                    output.push_str(cell.symbol());
                    output
                });
        assert!(screen.contains("Pushing"));

        for _ in 0..4 {
            terminal
                .draw(|frame| renderer.render(frame, &model))
                .unwrap();
        }
        assert_ne!(terminal.backend().buffer()[(0, 0)].fg, first_border);
    }

    #[test]
    fn renders_and_mouse_dismisses_a_bottom_right_toast() {
        let mut model = model();
        model.complete_operation(
            &OperationResult::Commit {
                hash: "a1b2c3d4e5".to_owned(),
            },
            model.snapshot.clone(),
        );
        let id = model.toasts[0].id;
        let mut renderer = Renderer::new();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| renderer.render(frame, &model))
            .unwrap();
        assert!(
            terminal.backend().buffer().content.iter().any(|cell| {
                cell.symbol().contains("Committed") || cell.fg == Color::LightGreen
            })
        );

        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 70,
            row: 26,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            renderer.map_event(&click, &model, Rect::new(0, 0, 100, 30)),
            Some(diffo_app::Message::DismissToast(id))
        );
    }

    fn diff_lines(
        renderer: &mut Renderer,
        model: &Model,
        first_row: usize,
    ) -> Vec<ratatui::text::Line<'static>> {
        for _ in 0..200 {
            let lines = renderer.diff_lines(model, 80, first_row, 100);
            if !renderer.is_preparing() {
                return lines;
            }
            sleep(Duration::from_millis(1));
        }
        panic!("diff preparation timed out");
    }

    #[test]
    fn renders_syntax_foregrounds_over_diff_backgrounds() {
        let mut renderer = Renderer::new();
        let lines = renderer.diff_lines(&model(), 80, 0, 100);
        assert!(!lines.is_empty());
        assert!(!renderer.is_preparing());
        let removed = &lines[1];
        let added = &lines[2];

        assert!(removed.spans.iter().any(|span| span.style.fg.is_some()));
        assert!(
            removed
                .spans
                .iter()
                .any(|span| { span.style.bg == Some(Color::Indexed(52)) })
        );
        assert!(
            added
                .spans
                .iter()
                .any(|span| { span.style.bg == Some(Color::Indexed(22)) })
        );
        assert_eq!(removed.spans[0].style.fg, Some(Color::LightRed));
        assert_eq!(added.spans[0].style.fg, Some(Color::LightGreen));
        assert!(
            removed.spans[1..]
                .iter()
                .all(|span| span.style.add_modifier.is_empty()),
            "syntax highlighting should not emit terminal font attributes"
        );
        assert_eq!(
            removed
                .spans
                .iter()
                .map(|span| span.content.chars().count())
                .sum::<usize>(),
            80
        );
    }

    #[test]
    fn prepares_large_diffs_in_the_background() {
        let mut model = model();
        let mut patch = String::from("@@ -0,0 +1,501 @@\n");
        for index in 0..501 {
            writeln!(patch, "+line {index}").unwrap();
        }
        model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
        let mut renderer = Renderer::new();

        let pending = renderer.diff_lines(&model, 80, 0, 100);
        assert!(pending.is_empty());
        assert!(renderer.is_preparing());

        let lines = diff_lines(&mut renderer, &model, 0);
        assert!(!lines.is_empty());
        assert!(!renderer.is_preparing());
    }

    #[test]
    fn keeps_previous_diff_visible_while_preparing() {
        let mut model = model();
        let mut renderer = Renderer::new();
        let previous = renderer.diff_lines(&model, 80, 0, 100);
        let mut patch = String::from("@@ -0,0 +1,501 @@\n");
        for index in 0..501 {
            writeln!(patch, "+line {index}").unwrap();
        }
        model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;

        let during_transition = renderer.diff_lines(&model, 80, 0, 100);

        assert_eq!(during_transition, previous);
        assert!(renderer.is_preparing());
    }

    #[test]
    fn anchors_the_first_visible_row_when_content_moves_above_it() {
        let mut inline_model = model();
        let patch = |prefix: &[&str]| {
            let mut patch = format!("@@ -0,0 +1,{} @@\n", prefix.len() + 40);
            for line in prefix {
                writeln!(patch, "+{line}").unwrap();
            }
            for index in 0..40 {
                writeln!(patch, "+stable line {index}").unwrap();
            }
            patch
        };
        inline_model.snapshot.files[0]
            .unstaged
            .as_mut()
            .unwrap()
            .text = patch(&[]);
        inline_model.diff_scroll = 12;
        let mut renderer = Renderer::new();
        let area = Rect::new(0, 0, 100, 30);
        let initial = renderer.prepare_frame(&inline_model, area);
        assert_eq!(initial.anchored_vertical_scroll, None);

        inline_model.snapshot.files[0]
            .unstaged
            .as_mut()
            .unwrap()
            .text = patch(&["inserted one", "inserted two", "inserted three"]);
        let changed = renderer.prepare_frame(&inline_model, area);

        assert_eq!(changed.anchored_vertical_scroll, Some(15));

        let mut side_model = model();
        side_model.diff_view_mode = DiffViewMode::SideBySide;
        side_model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch(&[]);
        side_model.diff_scroll = 12;
        let mut side_renderer = Renderer::new();
        side_renderer.prepare_frame(&side_model, area);
        side_model.snapshot.files[0].unstaged.as_mut().unwrap().text =
            patch(&["inserted one", "inserted two", "inserted three"]);

        let side_changed = side_renderer.prepare_frame(&side_model, area);

        assert_eq!(side_changed.anchored_vertical_scroll, Some(15));
    }

    #[test]
    fn uses_the_next_visible_row_when_the_anchor_was_deleted() {
        let mut model = model();
        let patch = |skip: Option<usize>| {
            let mut patch = String::from("@@ -0,0 +1,40 @@\n");
            for index in 0..40 {
                if skip != Some(index) {
                    writeln!(patch, "+stable line {index}").unwrap();
                }
            }
            patch
        };
        model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch(None);
        model.diff_scroll = 12;
        let mut renderer = Renderer::new();
        let area = Rect::new(0, 0, 100, 30);
        renderer.prepare_frame(&model, area);

        model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch(Some(11));
        let changed = renderer.prepare_frame(&model, area);

        assert_eq!(changed.anchored_vertical_scroll, Some(11));
    }

    #[test]
    fn renders_invalid_patches_as_raw_text() {
        let mut model = model();
        model.snapshot.files[0].unstaged.as_mut().unwrap().text =
            "diff --cc src/main.rs\n@@@ malformed\n+raw line\n".to_owned();
        let mut renderer = Renderer::new();

        let lines = renderer.diff_lines(&model, 80, 0, 100);

        assert_eq!(lines[0].to_string(), "diff --cc src/main.rs");
        assert_eq!(lines[2].to_string(), "+raw line");
        assert!(!renderer.is_preparing());
    }

    #[test]
    fn maps_inset_scrollbar_clicks_to_absolute_positions() {
        let mut model = model();
        let mut patch = String::from("@@ -0,0 +1,100 @@\n");
        for _ in 0..100 {
            writeln!(patch, "+{}", "x".repeat(200)).unwrap();
        }
        model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
        let mut renderer = Renderer::new();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| renderer.render(frame, &model))
            .unwrap();

        let vertical = renderer.scrollbars.vertical_area;
        let vertical_click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: vertical.right().saturating_sub(1),
            row: vertical.bottom().saturating_sub(1),
            modifiers: KeyModifiers::NONE,
        });
        assert!(matches!(
            renderer.map_event(&vertical_click, &model, Rect::new(0, 0, 100, 30)),
            Some(diffo_app::Message::SetDiffScroll(position)) if position > 0
        ));

        renderer.scrollbar_drag = None;
        let horizontal = renderer.scrollbars.horizontal_area;
        let horizontal_click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: horizontal.right().saturating_sub(1),
            row: horizontal.bottom().saturating_sub(1),
            modifiers: KeyModifiers::NONE,
        });
        let horizontal_maximum = renderer
            .scrollbars
            .columns
            .saturating_sub(renderer.scrollbars.viewport_columns);
        assert!(matches!(
            renderer.map_event(&horizontal_click, &model, Rect::new(0, 0, 100, 30)),
            Some(diffo_app::Message::SetDiffHorizontalScroll(position))
                if position == horizontal_maximum
        ));
    }

    #[test]
    fn renders_command_palette_over_the_diff() {
        let mut model = model();
        model.open_command_palette();
        let mut renderer = Renderer::new();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| renderer.render(frame, &model))
            .unwrap();

        let screen =
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .fold(String::new(), |mut output, cell| {
                    output.push_str(cell.symbol());
                    output
                });
        assert!(screen.contains("Command Palette"));
        assert!(screen.contains("Git: Pull"));
        assert!(screen.contains("esc close"));
    }

    #[test]
    fn command_palette_has_fixed_top_and_mouse_execution() {
        let mut model = model();
        model.open_command_palette();
        let area = Rect::new(0, 0, 100, 30);
        let (palette_area, results_area) = command_palette_layout(area);
        assert_eq!(palette_area.y, 6);
        assert_eq!(palette_area.height, 18);

        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: results_area.x,
            row: results_area.y.saturating_add(1),
            modifiers: KeyModifiers::NONE,
        });
        let mut renderer = Renderer::new();
        assert_eq!(
            renderer.map_event(&click, &model, area),
            Some(diffo_app::Message::ExecuteCommand(1))
        );
    }

    #[test]
    fn reuses_highlights_across_modes_and_invalidates_changed_patch() {
        let mut renderer = Renderer::new();
        let mut model = model();

        diff_lines(&mut renderer, &model, 0);
        model.diff_view_mode = DiffViewMode::SideBySide;
        diff_lines(&mut renderer, &model, 0);
        assert_eq!(renderer.highlight_computations, 1);

        model.snapshot.files[0]
            .unstaged
            .as_mut()
            .expect("unstaged diff")
            .text
            .push_str("\\ No newline at end of file\n");
        diff_lines(&mut renderer, &model, 0);
        assert_eq!(renderer.highlight_computations, 2);
    }

    #[test]
    fn lifts_low_contrast_theme_colors_on_diff_backgrounds() {
        let monokai_comment = Rgb {
            red: 117,
            green: 113,
            blue: 94,
        };
        for kind in [RowKind::Removed, RowKind::Added] {
            let adjusted = contrasting_foreground(monokai_comment, kind);
            let background = diff_background_rgb(kind).expect("changed row has a background");

            assert!(contrast_ratio(adjusted, background) >= 4.5);
        }
        assert_eq!(
            contrasting_foreground(monokai_comment, RowKind::Context),
            monokai_comment
        );
    }

    #[test]
    #[ignore = "manual performance measurement"]
    fn measures_large_diff_rendering() {
        let mut model = model();
        let mut patch = String::from("@@ -0,0 +1,100000 @@\n");
        for index in 0..100_000 {
            writeln!(patch, "+pub const ITEM_{index}: usize = {index};").unwrap();
        }
        model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
        let mut renderer = Renderer::new();

        let started = Instant::now();
        let loading = renderer.diff_lines(&model, 160, 0, 50);
        let enqueue = started.elapsed();
        assert!(loading.is_empty());
        let started = Instant::now();
        let lines = loop {
            let lines = renderer.diff_lines(&model, 160, 0, 50);
            if !renderer.is_preparing() {
                break lines;
            }
            sleep(Duration::from_millis(1));
        };
        let prepared = started.elapsed();
        let started = Instant::now();
        for row in (0..10_000).step_by(50) {
            assert_eq!(renderer.diff_lines(&model, 160, row, 50).len(), 50);
        }
        let cached = started.elapsed();

        eprintln!(
            "100k enqueue={enqueue:?} background_prepare={prepared:?} cached_200_viewports={cached:?}"
        );
        assert_eq!(lines.len(), 50);
        assert_eq!(renderer.highlight_computations, 0);
    }
}
