use diffo_ui::{disabled_control_style, modal_block, mouse_target_style, theme};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::ReviewActivity;

pub(super) struct ReviewHitAreas {
    pub stop_areas: Vec<(Rect, usize)>,
    pub generate_area: Rect,
}

#[expect(clippy::too_many_lines, reason = "renders one compact activity panel")]
pub(super) fn render_review(
    frame: &mut Frame,
    area: Rect,
    review: &ReviewActivity,
) -> ReviewHitAreas {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::CHROME))
        .title(" AI Review ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = Vec::new();
    let mut stop_rows = Vec::new();
    let mut generate_row = None;

    if let super::CodexAvailability::Unavailable(reason) = &review.availability {
        lines.push(Line::styled(
            "AI functionality is disabled.",
            disabled_control_style(),
        ));
        lines.push(Line::raw(""));
        lines.push(Line::styled(reason, disabled_control_style()));
    } else if let Some(active) = &review.active_request {
        let label = if active.cancellation.is_cancelled() {
            "Cancelling…"
        } else {
            "Generating review…  Enter: cancel"
        };
        lines.push(Line::styled(label, mouse_target_style()));
    } else if let Some(cached) = review.cached.as_ref() {
        let stale = review.stale();
        if stale {
            lines.push(Line::styled(
                "Stale review — Enter to regenerate",
                Style::default().fg(theme::WARNING),
            ));
            lines.push(Line::raw(""));
        }
        if let Some(error) = &review.failure {
            lines.push(Line::styled(
                error.clone(),
                Style::default().fg(theme::DANGER),
            ));
            lines.push(Line::raw(""));
        }
        for overview in &cached.result.overview {
            lines.push(Line::raw(overview.clone()));
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "Review map",
            Style::default().add_modifier(Modifier::BOLD),
        ));
        for (index, stop) in cached.result.stops.iter().enumerate() {
            let row = lines.len();
            stop_rows.push((row, index));
            let selected = index == review.selected_stop;
            let row_style = if stale {
                disabled_control_style()
            } else if selected {
                Style::default()
                    .fg(theme::TEXT)
                    .bg(theme::SELECTION_BACKGROUND)
            } else {
                Style::default().fg(theme::TEXT)
            };
            lines.push(Line::styled(
                format!("{}. {}", index + 1, stop.title),
                row_style,
            ));
            lines.push(Line::styled(
                format!("   {} · {}", stop.category.label(), stop.reason),
                if stale {
                    disabled_control_style()
                } else {
                    Style::default().fg(theme::CHROME)
                },
            ));
        }
    } else {
        lines.push(Line::raw(
            "Build an overview and a guided path through staged and unstaged changes.",
        ));
        lines.push(Line::raw(""));
        if let Some(error) = &review.failure {
            lines.push(Line::styled(
                error.clone(),
                Style::default().fg(theme::DANGER),
            ));
            lines.push(Line::raw(""));
        }
        generate_row = Some(lines.len());
        lines.push(Line::styled("[ Generate review ]", mouse_target_style()));
        lines.push(Line::raw("Sends the current diff through your Codex CLI."));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    let stop_areas = stop_rows
        .into_iter()
        .filter_map(|(row, index)| {
            let y = inner.y.saturating_add(u16::try_from(row).ok()?);
            (y < inner.bottom()).then_some((Rect::new(inner.x, y, inner.width, 2), index))
        })
        .collect();
    let generate_area = generate_row
        .and_then(|row| {
            let y = inner.y.saturating_add(u16::try_from(row).ok()?);
            (y < inner.bottom()).then_some(Rect::new(inner.x, y, inner.width, 1))
        })
        .unwrap_or_default();
    ReviewHitAreas {
        stop_areas,
        generate_area,
    }
}

pub(super) fn render_empty_diff(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new("Choose a review stop to open its hunk.")
            .block(modal_block("Diff"))
            .wrap(Wrap { trim: false }),
        area,
    );
}
