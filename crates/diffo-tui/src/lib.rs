use diffo_core::{AccessMode, ChangeKind, FileState};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

mod app;

pub use app::{App, ChangeArea, FileKey};
use app::{staged_files, unstaged_files};

pub fn render(frame: &mut Frame, app: &App) {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(frame.area());
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(vertical[0]);

    render_files(frame, panes[0], app);
    render_diff_placeholder(frame, panes[1], app);
    render_status(frame, vertical[1], app);
}

fn render_files(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let mut items = vec![group_header("Changes")];
    items.extend(
        unstaged_files(&app.snapshot)
            .map(|file| file_item(file, app.is_selected(&file.path, ChangeArea::Unstaged))),
    );
    items.push(group_header("Staged Changes"));
    items.extend(
        staged_files(&app.snapshot)
            .map(|file| file_item(file, app.is_selected(&file.path, ChangeArea::Staged))),
    );

    let title = if app.access_mode == AccessMode::ReadOnly {
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
    let mut state = ListState::default().with_selected(app.selected_row());
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

fn render_diff_placeholder(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let text = app.selected.as_ref().map_or_else(
        || "No file selected.".to_owned(),
        |selected| {
            format!(
                "{}\n\nFile diff comes next.",
                selected.path.to_string_lossy()
            )
        },
    );
    let pane =
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" File Diff "));
    frame.render_widget(pane, area);
}

fn render_status(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let text = app
        .error
        .as_deref()
        .unwrap_or(if app.access_mode == AccessMode::ReadOnly {
            " j/k: select  q: quit  read-only "
        } else {
            " j/k: select  s: stage  u: unstage  a: stage all  q: quit "
        });
    let style = if app.error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}
