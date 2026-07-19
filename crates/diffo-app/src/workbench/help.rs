use diffo_ui::{design, theme};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table},
};

const BUILD_TAG: &str = env!("DIFFO_BUILD_TAG");
const BUILD_SHA: &str = env!("DIFFO_BUILD_SHA");

pub(super) fn render(frame: &mut Frame, content_area: Rect, rows: Vec<(String, &'static str)>) {
    let area = layout(content_area);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::CHROME))
        .title(" Help ");
    let inner = block.inner(area).inner(design::DIALOG_INSET);
    frame.render_widget(block, area);
    let sections = Layout::vertical([
        Constraint::Min(design::SINGLE_LINE_HEIGHT),
        Constraint::Length(design::SINGLE_LINE_HEIGHT),
    ])
    .split(inner);
    let rows = rows.into_iter().map(|(keys, description)| {
        Row::new([
            Cell::from(keys).style(
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
            Cell::from(description).style(Style::default().fg(theme::TEXT)),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(design::HELP_SHORTCUT_COLUMN_WIDTH),
            Constraint::Min(design::HELP_ACTION_MIN_WIDTH),
        ],
    )
    .header(
        Row::new(["Shortcut", "Action"]).style(
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .column_spacing(design::HELP_COLUMN_GAP);
    frame.render_widget(table, sections[0]);
    let build = format!("tag {BUILD_TAG} · sha {BUILD_SHA}");
    let footer = Layout::horizontal([
        Constraint::Min(design::SINGLE_LINE_HEIGHT),
        Constraint::Length(u16::try_from(build.chars().count()).unwrap_or(u16::MAX)),
    ])
    .split(sections[1]);
    let footer_style = Style::default().fg(theme::CHROME);
    frame.render_widget(Paragraph::new("Esc: close").style(footer_style), footer[0]);
    frame.render_widget(
        Paragraph::new(build)
            .style(footer_style)
            .alignment(Alignment::Right),
        footer[1],
    );
}

#[must_use]
fn layout(area: Rect) -> Rect {
    let width = design::HELP_WIDTH.resolve(area.width);
    let top = area
        .y
        .saturating_add(area.height.saturating_mul(design::HELP_TOP_PERCENT) / 100);
    let height = design::HELP_MAX_HEIGHT.min(area.bottom().saturating_sub(top));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        top,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn help_layout_uses_the_shared_dialog_contract() {
        assert_eq!(layout(Rect::new(5, 3, 100, 30)), Rect::new(15, 3, 80, 30));
    }

    #[test]
    fn help_footer_shows_the_build_tag_and_sha() {
        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, area, Vec::new()))
            .unwrap();

        let help_area = layout(area);
        let footer_y = help_area.bottom() - 3;
        let footer = (help_area.x..help_area.right())
            .map(|x| terminal.backend().buffer()[(x, footer_y)].symbol())
            .collect::<String>();
        assert!(footer.contains("Esc: close"), "{footer}");
        assert!(
            footer.contains(&format!("tag {BUILD_TAG} · sha {BUILD_SHA}")),
            "{footer}"
        );
    }
}
