use diffo_app::Activity;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
};

pub const ACTIVITY_BAR_WIDTH: u16 = 5;
const ACTIVITY_BUTTON_HEIGHT: u16 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchAreas {
    pub activity_bar: Rect,
    pub content: Rect,
}

#[must_use]
pub fn workbench_areas(area: Rect) -> WorkbenchAreas {
    let areas = Layout::horizontal([Constraint::Length(ACTIVITY_BAR_WIDTH), Constraint::Min(0)])
        .split(area);
    WorkbenchAreas {
        activity_bar: areas[0],
        content: areas[1],
    }
}

#[must_use]
pub fn activity_at_position(area: Rect, column: u16, row: u16) -> Option<Activity> {
    let bar = workbench_areas(area).activity_bar;
    if column < bar.x
        || column >= bar.right().saturating_sub(1)
        || row < bar.y
        || row >= bar.bottom()
    {
        return None;
    }
    match row.saturating_sub(bar.y) / ACTIVITY_BUTTON_HEIGHT {
        0 => Some(Activity::Explorer),
        1 => Some(Activity::Search),
        2 => Some(Activity::Diff),
        _ => None,
    }
}

pub fn render_activity_bar(frame: &mut Frame, area: Rect, active: Activity) {
    let bar = workbench_areas(area).activity_bar;
    frame.render_widget(
        Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(Color::DarkGray)),
        bar,
    );
    for (index, (activity, icon)) in [
        (Activity::Explorer, "▤"),
        (Activity::Search, "⌕"),
        (Activity::Diff, "≠"),
    ]
    .into_iter()
    .enumerate()
    {
        let y = bar.y.saturating_add(
            u16::try_from(index)
                .unwrap_or(u16::MAX)
                .saturating_mul(ACTIVITY_BUTTON_HEIGHT),
        );
        if y >= bar.bottom() {
            break;
        }
        let button = Rect::new(
            bar.x,
            y,
            bar.width.saturating_sub(1),
            ACTIVITY_BUTTON_HEIGHT.min(bar.bottom().saturating_sub(y)),
        );
        let selected = activity == active;
        let style = if selected {
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        frame.render_widget(
            Paragraph::new(icon)
                .alignment(Alignment::Center)
                .style(style),
            Rect::new(button.x, button.y.saturating_add(1), button.width, 1),
        );
        if selected && button.width > 0 && button.height > 1 {
            frame.render_widget(
                Paragraph::new("▌").style(style),
                Rect::new(button.x, button.y.saturating_add(1), 1, 1),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn reserves_the_full_left_edge_and_maps_buttons() {
        let area = Rect::new(2, 4, 100, 30);
        let areas = workbench_areas(area);
        assert_eq!(areas.activity_bar, Rect::new(2, 4, 5, 30));
        assert_eq!(areas.content, Rect::new(7, 4, 95, 30));
        assert_eq!(activity_at_position(area, 3, 5), Some(Activity::Explorer));
        assert_eq!(activity_at_position(area, 3, 8), Some(Activity::Search));
        assert_eq!(activity_at_position(area, 3, 11), Some(Activity::Diff));
        assert_eq!(activity_at_position(area, 6, 5), None);
        assert_eq!(activity_at_position(area, 3, 14), None);
    }

    #[test]
    fn narrow_areas_do_not_underflow() {
        let areas = workbench_areas(Rect::new(0, 0, 3, 2));
        assert_eq!(areas.activity_bar.width, 3);
        assert_eq!(areas.content.width, 0);
    }

    #[test]
    fn renders_icons_and_marks_the_active_activity() {
        let backend = TestBackend::new(20, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_activity_bar(frame, frame.area(), Activity::Search))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let cell = |symbol: &str| {
            buffer
                .content
                .iter()
                .find(|cell| cell.symbol() == symbol)
                .unwrap()
        };

        assert_eq!(cell("▤").fg, Color::Gray);
        assert_eq!(cell("≠").fg, Color::Gray);
        assert_eq!(cell("▌").fg, Color::LightCyan);
        assert_eq!(cell("⌕").fg, Color::LightCyan);
        assert!(cell("⌕").modifier.contains(Modifier::BOLD));
        assert_eq!(buffer[(4, 0)].fg, Color::DarkGray);
    }
}
