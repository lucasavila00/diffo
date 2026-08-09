use diffo_ui::{disabled_control_style, modal_block, mouse_target_style, theme};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::{AskState, ReviewActivity};

pub(super) struct ReviewHitAreas {
    pub stop_areas: Vec<(Rect, usize)>,
    pub generate_area: Rect,
}

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
        } else if active.kind == super::ActiveRequestKind::Ask {
            "Answering…  Enter: cancel"
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
        for overview in &cached.result.overview {
            lines.push(Line::raw(overview.clone()));
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!(
                "Review map  {}/{}",
                review.visited.len(),
                cached.result.stops.len()
            ),
            Style::default().add_modifier(Modifier::BOLD),
        ));
        for (index, stop) in cached.result.stops.iter().enumerate() {
            let row = lines.len();
            stop_rows.push((row, index));
            let marker = if review.visited.contains(&stop.primary_hunk_id) {
                "✓"
            } else {
                "·"
            };
            let selected = index == review.selected_stop;
            let style = if stale {
                disabled_control_style()
            } else if selected {
                Style::default()
                    .fg(theme::TEXT)
                    .bg(theme::SELECTION_BACKGROUND)
            } else {
                Style::default().fg(theme::TEXT)
            };
            lines.push(Line::styled(
                format!("{marker} {}. {}", index + 1, stop.title),
                style,
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
        if !stale {
            lines.push(Line::raw(""));
            lines.push(Line::styled("/: Ask the diff", mouse_target_style()));
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

    match &review.ask {
        AskState::Closed => {}
        AskState::Editing { question } => {
            lines.push(Line::raw(""));
            lines.push(Line::styled("Ask the diff", mouse_target_style()));
            lines.push(Line::from(vec![
                Span::raw("> "),
                Span::raw(question.clone()),
                Span::styled("▏", mouse_target_style()),
            ]));
            lines.push(Line::styled(
                "Enter: ask · Esc: close",
                Style::default().fg(theme::CHROME),
            ));
        }
        AskState::Running { question } => {
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                format!("Asking: {question}"),
                Style::default().fg(theme::CHROME),
            ));
        }
        AskState::Answered {
            question,
            answer,
            selected_link,
        } => {
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                format!("Q: {question}"),
                Style::default().add_modifier(Modifier::BOLD),
            ));
            for line in &answer.text {
                lines.push(Line::raw(line.clone()));
            }
            for (index, id) in answer.hunk_ids.iter().enumerate() {
                lines.push(Line::styled(
                    format!("  {id}"),
                    if index == *selected_link {
                        mouse_target_style()
                    } else {
                        Style::default().fg(theme::TEXT)
                    },
                ));
            }
            lines.push(Line::styled(
                "j/k: link · Enter: open · Esc: close",
                Style::default().fg(theme::CHROME),
            ));
        }
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
