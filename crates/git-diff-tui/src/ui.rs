use ratatui::{
    Frame,
    style::{Color, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App) {
    let lines = app.diff.lines().map(styled_diff_line).collect::<Vec<_>>();
    let body = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Git diff ")
                .title_bottom(" j/k: scroll  PgUp/PgDn: page  q: quit "),
        )
        .scroll((app.scroll.try_into().unwrap_or(u16::MAX), 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(body, frame.area());
}

fn styled_diff_line(line: &str) -> Line<'_> {
    let color = if line.starts_with('+') && !line.starts_with("+++") {
        Color::Green
    } else if line.starts_with('-') && !line.starts_with("---") {
        Color::Red
    } else if line.starts_with("@@") {
        Color::Cyan
    } else {
        Color::Reset
    };

    Line::styled(line, Style::default().fg(color))
}
