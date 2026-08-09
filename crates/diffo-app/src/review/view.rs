use diffo_ui::{
    disabled_control_style, modal_block, mouse_target_style, terminal_safe_text, theme,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::diff::ChangeArea;

use super::{CachedReview, CodexAvailability, ReviewActivity};

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

    let footer = footer_lines(review);
    let footer_height = u16::try_from(footer.len())
        .unwrap_or(u16::MAX)
        .min(inner.height);
    let sections =
        Layout::vertical([Constraint::Min(0), Constraint::Length(footer_height)]).split(inner);
    let body_area = sections[0];
    let footer_area = sections[1];

    let (stop_areas, generate_area) = if let Some(cached) = review.ready() {
        (
            render_ready(frame, body_area, review, cached),
            Rect::default(),
        )
    } else {
        (Vec::new(), render_state(frame, body_area, review))
    };
    if !footer.is_empty() {
        frame.render_widget(Paragraph::new(footer), footer_area);
    }

    ReviewHitAreas {
        stop_areas,
        generate_area,
    }
}

fn render_state(frame: &mut Frame, area: Rect, review: &ReviewActivity) -> Rect {
    let mut lines = Vec::new();
    let mut action = None;
    if let CodexAvailability::Unavailable(reason) = &review.availability {
        lines.push(Line::styled(
            "AI Review is unavailable.",
            disabled_control_style(),
        ));
        lines.push(Line::raw(""));
        lines.push(Line::styled(reason.clone(), disabled_control_style()));
    } else if let Some(active) = &review.active_request {
        let label = if active.cancelling {
            "Cancelling review…"
        } else {
            "Building your review…"
        };
        lines.push(Line::styled(label, mouse_target_style()));
        lines.push(Line::raw(""));
        lines.push(Line::raw(active.progress.as_ref().map_or_else(
            || "Preparing changes and starting Codex…".to_owned(),
            super::ReviewProgress::description,
        )));
        if let Some(progress) = &active.progress {
            lines.push(Line::raw(progress_files(progress)));
        }
        lines.push(Line::raw("0 review steps ready"));
        lines.push(Line::raw("Results appear here as each part finishes."));
    } else if review.stale() {
        lines.push(Line::styled(
            "Your changes changed after this review.",
            Style::default().fg(theme::WARNING),
        ));
        lines.push(Line::raw("Build a fresh review before continuing."));
        lines.push(Line::raw(""));
        if let Some(error) = &review.failure {
            lines.push(Line::styled(
                error.clone(),
                Style::default().fg(theme::DANGER),
            ));
            lines.push(Line::raw(""));
        }
        action = Some("[ Refresh review ]  Enter");
    } else if !review.has_changes() {
        lines.push(Line::styled(
            "Nothing to review.",
            Style::default().add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::raw(""));
        lines.push(Line::raw("Make a change, then come back."));
    } else {
        lines.push(Line::raw("Review your changes in a suggested order."));
        lines.push(Line::raw(""));
        lines.push(Line::raw(
            "Codex summarizes the work, then opens one change in one file at a time.",
        ));
        lines.push(Line::raw(""));
        if let Some(error) = &review.failure {
            lines.push(Line::styled(
                error.clone(),
                Style::default().fg(theme::DANGER),
            ));
            lines.push(Line::raw(""));
            action = Some("[ Try again ]  Enter");
        } else {
            action = Some("[ Start review ]  Enter");
        }
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
    let Some(label) = action else {
        return Rect::default();
    };
    let action_area = Rect::new(
        area.x,
        area.bottom().saturating_sub(1),
        area.width,
        u16::from(area.height > 0),
    );
    frame.render_widget(
        Paragraph::new(Line::styled(label, mouse_target_style())),
        action_area,
    );
    action_area
}

#[expect(clippy::too_many_lines, reason = "renders one guided review panel")]
fn render_ready(
    frame: &mut Frame,
    area: Rect,
    review: &ReviewActivity,
    cached: &CachedReview,
) -> Vec<(Rect, usize)> {
    let Some(stop) = cached.result.stops.get(review.selected_stop) else {
        return Vec::new();
    };
    let Some(target) = cached.request.hunk(&stop.primary_hunk_id) else {
        return Vec::new();
    };
    let mut y = area.y;
    render_row(
        frame,
        area,
        &mut y,
        Line::styled("Summary", Style::default().add_modifier(Modifier::BOLD)),
    );
    let overview_height = 3.min(area.bottom().saturating_sub(y));
    if overview_height > 0 {
        frame.render_widget(
            Paragraph::new(
                cached
                    .result
                    .overview
                    .iter()
                    .map(|line| Line::raw(line.clone()))
                    .collect::<Vec<_>>(),
            )
            .wrap(Wrap { trim: false }),
            Rect::new(area.x, y, area.width, overview_height),
        );
        y = y.saturating_add(overview_height);
    }
    y = y.saturating_add(1).min(area.bottom());

    let count = cached.result.stops.len();
    render_row(
        frame,
        area,
        &mut y,
        Line::styled(
            "Review order · one change per step",
            Style::default().add_modifier(Modifier::BOLD),
        ),
    );
    let remaining = area.bottom().saturating_sub(y);
    let capacity =
        usize::from(remaining.saturating_sub(11).max(u16::from(remaining > 0))).min(count);
    let start = review
        .selected_stop
        .saturating_sub(capacity.saturating_sub(1) / 2)
        .min(count.saturating_sub(capacity));
    let stop_areas = cached
        .result
        .stops
        .iter()
        .enumerate()
        .skip(start)
        .take(capacity)
        .filter_map(|(index, stop)| {
            let row = next_row(area, &mut y)?;
            let selected = index == review.selected_stop;
            let style = if selected {
                Style::default()
                    .fg(theme::TEXT)
                    .bg(theme::SELECTION_BACKGROUND)
            } else {
                Style::default().fg(theme::TEXT)
            };
            let marker = if selected { ">" } else { " " };
            frame.render_widget(
                Paragraph::new(Line::styled(
                    format!("{marker} {}. {}", index + 1, stop.title),
                    style,
                )),
                row,
            );
            Some((row, index))
        })
        .collect();
    y = y.saturating_add(1).min(area.bottom());

    let final_step = review.active_request.is_none() && review.selected_stop + 1 == count;
    let progress = format!("Selected change {} of {count}", review.selected_stop + 1);
    render_row(
        frame,
        area,
        &mut y,
        Line::styled(progress, Style::default().add_modifier(Modifier::BOLD)),
    );
    render_row(
        frame,
        area,
        &mut y,
        Line::styled(
            terminal_safe_text(&stop.title),
            Style::default().add_modifier(Modifier::BOLD),
        ),
    );
    let state = match target.file.area {
        ChangeArea::Staged => "Staged",
        ChangeArea::Unstaged => "Unstaged",
    };
    render_row(
        frame,
        area,
        &mut y,
        Line::styled(
            format!(
                "File · {}",
                terminal_safe_text(&target.file.path.to_string_lossy())
            ),
            Style::default().fg(theme::CHROME),
        ),
    );
    render_row(
        frame,
        area,
        &mut y,
        Line::styled(
            format!("Focus · one change · {}", stop.category.label()),
            Style::default().fg(theme::CHROME),
        ),
    );
    render_row(
        frame,
        area,
        &mut y,
        Line::styled(
            format!("File state · {state}"),
            Style::default().fg(theme::CHROME),
        ),
    );
    let completion = final_step
        .then(|| Line::styled("End of guided review", Style::default().fg(theme::SUCCESS)));
    render_row(
        frame,
        area,
        &mut y,
        completion.unwrap_or_else(|| Line::raw("")),
    );
    y = y.saturating_add(1).min(area.bottom());
    render_row(
        frame,
        area,
        &mut y,
        Line::styled(
            "Why this matters",
            Style::default().add_modifier(Modifier::BOLD),
        ),
    );
    let reason_height = 3.min(area.bottom().saturating_sub(y));
    if reason_height > 0 {
        frame.render_widget(
            Paragraph::new(stop.reason.clone()).wrap(Wrap { trim: false }),
            Rect::new(area.x, y, area.width, reason_height),
        );
    }
    stop_areas
}

fn render_row(frame: &mut Frame, area: Rect, y: &mut u16, line: Line<'static>) {
    if let Some(row) = next_row(area, y) {
        frame.render_widget(Paragraph::new(line), row);
    }
}

fn next_row(area: Rect, y: &mut u16) -> Option<Rect> {
    if *y >= area.bottom() {
        return None;
    }
    let row = Rect::new(area.x, *y, area.width, 1);
    *y = y.saturating_add(1);
    Some(row)
}

fn footer_lines(review: &ReviewActivity) -> Vec<Line<'static>> {
    if !review.available() || !review.has_changes() {
        return Vec::new();
    }
    if review.ready().is_some() {
        let mut lines = Vec::new();
        if let Some(active) = &review.active_request {
            let count = review.ready().map_or(0, |cached| cached.result.stops.len());
            let steps = if count == 1 { "step" } else { "steps" };
            lines.push(Line::styled(
                active.progress.as_ref().map_or_else(
                    || "Preparing the next part…".to_owned(),
                    super::ReviewProgress::description,
                ),
                Style::default().fg(theme::INFORMATION),
            ));
            if let Some(progress) = &active.progress {
                lines.push(Line::styled(
                    progress_files(progress),
                    Style::default().fg(theme::CHROME),
                ));
            }
            lines.push(Line::styled(
                format!("{count} review {steps} ready · j / k  Previous / next change"),
                Style::default().fg(theme::CHROME),
            ));
            if active.cancelling {
                lines.push(Line::styled(
                    "Cancelling review…",
                    Style::default().fg(theme::WARNING),
                ));
            } else {
                lines.push(Line::styled("Enter  Cancel review", mouse_target_style()));
            }
            return lines;
        }
        lines.push(Line::styled(
            "j / k  Previous / next change",
            Style::default().fg(theme::CHROME),
        ));
        if let Some(file) = review.active_file() {
            let staging = match file.area {
                ChangeArea::Staged => "Space  Unstage file",
                ChangeArea::Unstaged => "Space  Stage file",
            };
            lines.push(Line::styled(staging, Style::default().fg(theme::CHROME)));
        }
        lines.push(Line::styled(
            "i  Commit staged work",
            Style::default().fg(theme::CHROME),
        ));
        return lines;
    }
    if review.active_request.is_some() {
        return vec![Line::styled("Enter  Cancel review", mouse_target_style())];
    }
    vec![
        Line::styled(
            "How it works",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            "j / k  Previous / next change",
            Style::default().fg(theme::CHROME),
        ),
        Line::styled("Space  Stage / unstage", Style::default().fg(theme::CHROME)),
        Line::styled("       the whole file", Style::default().fg(theme::CHROME)),
        Line::styled("i  Commit staged work", Style::default().fg(theme::CHROME)),
    ]
}

pub(super) fn render_empty_diff(frame: &mut Frame, area: Rect, review: &ReviewActivity) {
    let message = if let CodexAvailability::Unavailable(reason) = &review.availability {
        format!("AI Review is unavailable.\n\n{reason}")
    } else if let Some(active) = &review.active_request {
        let progress = active.progress.as_ref().map_or_else(
            || "Preparing changes and starting Codex…".to_owned(),
            super::ReviewProgress::description,
        );
        let files = active
            .progress
            .as_ref()
            .map(progress_files)
            .unwrap_or_default();
        format!("{progress}\n{files}\n\nThe first suggested change will open here.")
    } else if review.stale() {
        "Your changes changed after this review.\n\nPress Enter to refresh it.".to_owned()
    } else if !review.has_changes() {
        "Nothing to review.\n\nMake a change, then come back.".to_owned()
    } else if review.failure.is_some() {
        "The review could not be built.\n\nPress Enter to try again.".to_owned()
    } else {
        "Press Enter to start.\n\nThe first suggested change will open here.".to_owned()
    };
    frame.render_widget(
        Paragraph::new(message)
            .block(modal_block("Diff"))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn progress_files(progress: &super::ReviewProgress) -> String {
    let files = progress
        .files
        .iter()
        .map(|path| terminal_safe_text(&path.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("Files now: {files}")
}
