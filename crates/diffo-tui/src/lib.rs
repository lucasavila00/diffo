use diffo_app::{ChangeArea, DiffViewMode, FileKey, Model};
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
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
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
            #[cfg(test)]
            highlight_computations: 0,
        }
    }

    pub fn render(&mut self, frame: &mut Frame, model: &Model) {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(frame.area());
        let panes = horizontal_panes(vertical[0], model.file_pane_percent);

        render_files(frame, panes[0], model);
        self.render_diff(frame, panes[1], model);
        render_status(frame, vertical[1], model);
        render_command_palette(frame, model);
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
        if model.command_palette.is_some() {
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
                    self.highlighted = Some(cache);
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
                self.highlighted = Some(cache);
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
                area.height.saturating_sub(2),
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
            let mut state = ScrollbarState::new(rows)
                .viewport_content_length(viewport_rows)
                .position(model.diff_scroll);
            frame.render_stateful_widget(scrollbar, self.scrollbars.vertical_area, &mut state);
        }
        if columns > viewport_columns {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
                .begin_symbol(None)
                .end_symbol(None)
                .thumb_style(Style::default().fg(Color::Cyan));
            let mut state = ScrollbarState::new(columns)
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

fn render_command_palette(frame: &mut Frame, model: &Model) {
    let Some(palette) = model.command_palette.as_ref() else {
        return;
    };
    let commands = palette.matches();
    let area = centered_palette_area(frame.area(), commands.len());
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
    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);
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
    frame.render_stateful_widget(list, sections[2], &mut state);
    frame.render_widget(
        Paragraph::new("type to search · ↑↓ select · esc close")
            .style(Style::default().fg(Color::DarkGray)),
        sections[3],
    );
}

fn centered_palette_area(area: Rect, command_count: usize) -> Rect {
    let width = (area.width.saturating_mul(7) / 10).clamp(30.min(area.width), 80.min(area.width));
    let wanted_height = u16::try_from(command_count.saturating_add(5)).unwrap_or(u16::MAX);
    let height = wanted_height.clamp(7.min(area.height), 18.min(area.height));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 4,
        width,
        height,
    )
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
    let panes = horizontal_panes(vertical[0], model.file_pane_percent);
    let groups = file_group_areas(panes[0]);
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
    let groups = file_group_areas(area);
    render_file_group(
        frame,
        groups[0],
        " Staged Changes ",
        staged_files(&model.snapshot),
        ChangeArea::Staged,
        model,
    );
    let changes_title = if model.access_mode == AccessMode::ReadOnly {
        " Changes · read-only "
    } else {
        " Changes · [a] Stage All "
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
    let items = files
        .into_iter()
        .map(|file| file_item(file, model.is_selected(&file.path, change_area)));
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

fn file_item(file: &FileState, selected: bool) -> ListItem<'static> {
    let marker = match file.kind {
        ChangeKind::Added | ChangeKind::Untracked => "A",
        ChangeKind::Modified => "M",
        ChangeKind::Deleted => "D",
        ChangeKind::Renamed => "R",
        ChangeKind::Copied => "C",
        ChangeKind::Conflicted => "U",
    };
    let line = format!("{marker}  {}", file.path.display());
    let style = if selected {
        Style::default().fg(Color::White)
    } else {
        Style::default()
    };
    ListItem::new(Line::styled(line, style))
}

fn inline_line(row: &RenderLine, highlighted: &HighlightedDiff, width: usize) -> Line<'static> {
    let prefix = match row.kind {
        RowKind::Removed => "-",
        RowKind::Added => "+",
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
        RowKind::Header | RowKind::Meta => None,
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
        RowKind::Header | RowKind::Context | RowKind::Changed | RowKind::Meta => None,
    }
}

fn row_style(kind: RowKind) -> Style {
    match kind {
        RowKind::Header => Style::default().fg(Color::Cyan),
        RowKind::Removed => Style::default().fg(Color::Red),
        RowKind::Added => Style::default().fg(Color::Green),
        RowKind::Meta => Style::default().fg(Color::Yellow),
        RowKind::Context | RowKind::Changed => Style::default(),
    }
}

fn render_status(frame: &mut Frame, area: ratatui::layout::Rect, model: &Model) {
    let text = if let Some(error) = model.error.as_deref() {
        error.to_owned()
    } else if model.resizing_file_pane {
        format!(
            " Resizing file pane: {}% · release mouse to finish ",
            model.file_pane_percent
        )
    } else {
        input::help_text(model.access_mode)
    };
    let style = if model.error.is_some() {
        Style::default().fg(Color::Red)
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
    use diffo_core::{AccessMode, ChangeKind, FileDiff, FileState, RepositorySnapshot};
    use diffo_diff::RowKind;
    use diffo_highlight::Rgb;
    use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Color};

    use super::{Renderer, contrast_ratio, contrasting_foreground, diff_background_rgb};

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
            column: horizontal.right().saturating_sub(2),
            row: horizontal.bottom().saturating_sub(1),
            modifiers: KeyModifiers::NONE,
        });
        assert!(matches!(
            renderer.map_event(&horizontal_click, &model, Rect::new(0, 0, 100, 30)),
            Some(diffo_app::Message::SetDiffHorizontalScroll(position)) if position > 0
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
