use diffo_ui::{design, theme};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table},
};

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
    frame.render_widget(
        Paragraph::new("Esc: close").style(Style::default().fg(theme::CHROME)),
        sections[1],
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

    #[test]
    fn help_layout_uses_the_shared_dialog_contract() {
        assert_eq!(layout(Rect::new(5, 3, 100, 30)), Rect::new(15, 3, 80, 30));
    }
}
