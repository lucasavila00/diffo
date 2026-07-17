use diffo_app::{ChangeArea, DiffViewMode, FileKey, Model};
use diffo_core::{AccessMode, ChangeKind, FileState, RepositorySnapshot};
use diffo_diff::{
    RenderLine, RowKind, SideBySideRow, inline_rows, parse_unified_patch, side_by_side_rows,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

mod input;

pub use input::map_event;

pub fn render(frame: &mut Frame, model: &Model) {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(frame.area());
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(vertical[0]);

    render_files(frame, panes[0], model);
    render_diff_placeholder(frame, panes[1], model);
    render_status(frame, vertical[1], model);
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
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(vertical[0]);
    let files = panes[0];
    let inside_files = column > files.x
        && column < files.x.saturating_add(files.width).saturating_sub(1)
        && row > files.y
        && row < files.y.saturating_add(files.height).saturating_sub(1);
    if !inside_files {
        return None;
    }

    file_at_display_row(model, usize::from(row - files.y - 1))
}

fn file_at_display_row(model: &Model, row: usize) -> Option<FileKey> {
    let unstaged = unstaged_files(&model.snapshot).collect::<Vec<_>>();
    if (1..=unstaged.len()).contains(&row) {
        return Some(FileKey {
            path: unstaged[row - 1].path.clone(),
            area: ChangeArea::Unstaged,
        });
    }
    let staged_index = row.checked_sub(unstaged.len() + 2)?;
    staged_files(&model.snapshot)
        .nth(staged_index)
        .map(|file| FileKey {
            path: file.path.clone(),
            area: ChangeArea::Staged,
        })
}

fn render_files(frame: &mut Frame, area: ratatui::layout::Rect, model: &Model) {
    let mut items = vec![group_header("Changes")];
    items.extend(
        unstaged_files(&model.snapshot)
            .map(|file| file_item(file, model.is_selected(&file.path, ChangeArea::Unstaged))),
    );
    items.push(group_header("Staged Changes"));
    items.extend(
        staged_files(&model.snapshot)
            .map(|file| file_item(file, model.is_selected(&file.path, ChangeArea::Staged))),
    );

    let title = if model.access_mode == AccessMode::ReadOnly {
        " Files · read-only "
    } else {
        " Files · [a] Stage All "
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    let mut state = ListState::default().with_selected(model.selected_row());
    frame.render_stateful_widget(list, area, &mut state);
}

fn group_header(name: &str) -> ListItem<'_> {
    ListItem::new(Line::styled(
        name,
        Style::default().add_modifier(Modifier::BOLD),
    ))
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

fn render_diff_placeholder(frame: &mut Frame, area: ratatui::layout::Rect, model: &Model) {
    let mode = match model.diff_view_mode {
        DiffViewMode::Inline => "Inline",
        DiffViewMode::SideBySide => "Side by side",
    };
    let lines = diff_lines(model, area.width.saturating_sub(2));
    let pane = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" File Diff · {mode} ")),
        )
        .scroll((
            model.diff_scroll.try_into().unwrap_or(u16::MAX),
            model.diff_horizontal_scroll.try_into().unwrap_or(u16::MAX),
        ));
    frame.render_widget(pane, area);
}

fn diff_lines(model: &Model, width: u16) -> Vec<Line<'static>> {
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
    let Ok(document) = parse_unified_patch(&diff.text) else {
        return diff
            .text
            .lines()
            .map(|line| Line::raw(line.to_owned()))
            .collect();
    };
    if document.binary {
        return vec![Line::raw("Binary file changed.")];
    }

    match model.diff_view_mode {
        DiffViewMode::Inline => inline_rows(&document).iter().map(inline_line).collect(),
        DiffViewMode::SideBySide => {
            let column_width = usize::from(width.saturating_sub(3) / 2);
            side_by_side_rows(&document)
                .iter()
                .map(|row| side_by_side_line(row, column_width))
                .collect()
        }
    }
}

fn inline_line(row: &RenderLine) -> Line<'static> {
    let prefix = match row.kind {
        RowKind::Removed => "-",
        RowKind::Added => "+",
        RowKind::Header => "@",
        _ => " ",
    };
    let number = row
        .number
        .map_or_else(|| "    ".to_owned(), |number| format!("{number:>4}"));
    Line::styled(
        format!("{number} {prefix} {}", row.text),
        row_style(row.kind),
    )
}

fn side_by_side_line(row: &SideBySideRow, column_width: usize) -> Line<'static> {
    let old = format_cell(row.old.as_ref(), column_width);
    let new = format_cell(row.new.as_ref(), column_width);
    let old_style = row
        .old
        .as_ref()
        .map_or_else(Style::default, |line| row_style(line.kind));
    let new_style = row
        .new
        .as_ref()
        .map_or_else(Style::default, |line| row_style(line.kind));
    Line::from(vec![
        Span::styled(old, old_style),
        Span::raw(" │ "),
        Span::styled(new, new_style),
    ])
}

fn format_cell(line: Option<&RenderLine>, width: usize) -> String {
    let Some(line) = line else {
        return " ".repeat(width);
    };
    let number = line
        .number
        .map_or_else(|| "    ".to_owned(), |number| format!("{number:>4}"));
    let value = format!("{number} {}", line.text);
    let clipped = value.chars().take(width).collect::<String>();
    format!("{clipped:<width$}")
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
    let text = model
        .error
        .as_deref()
        .unwrap_or(if model.access_mode == AccessMode::ReadOnly {
            input::READ_ONLY_HELP
        } else {
            input::READ_WRITE_HELP
        });
    let style = if model.error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
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
