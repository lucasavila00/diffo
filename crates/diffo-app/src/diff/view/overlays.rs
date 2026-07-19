use crate::diff::{
    Alignment, Block, Borders, Clear, Constraint, Frame, Layout, Line, Model, Paragraph, Rect,
    Style, Toast, ToastKind, terminal_safe_text,
};
use diffo_ui::{
    command_progress_style, design, disabled_control_style, enabled_control_style, interaction,
    theme,
};

#[derive(Clone, Copy)]
pub struct CommandProgress<'a> {
    pub label: &'a str,
    pub cancelling: bool,
    pub animation_tick: usize,
}

pub fn render_command_progress(
    frame: &mut Frame,
    progress: CommandProgress<'_>,
    content_area: Rect,
) {
    const SPINNER: [&str; 4] = ["◐", "◓", "◑", "◒"];

    let Some(area) = command_progress_area(content_area) else {
        return;
    };
    let label = if progress.cancelling {
        format!("Cancelling {}…", progress.label.to_lowercase())
    } else {
        format!(
            "{} {}…",
            SPINNER[(progress.animation_tick / 2) % SPINNER.len()],
            progress.label
        )
    };
    let label = terminal_safe_text(&label);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(label)
            .style(command_progress_style(progress.animation_tick))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::CHROME))
                    .title(
                        Line::styled(interaction::DISMISS, enabled_control_style())
                            .alignment(Alignment::Right),
                    ),
            ),
        area,
    );
}

#[must_use]
pub fn command_cancel_at_position(area: Rect, column: u16, row: u16) -> bool {
    command_progress_area(area).is_some_and(|progress| {
        row == progress.y && column == progress.right().saturating_sub(design::INLINE_GAP)
    })
}

fn command_progress_area(area: Rect) -> Option<Rect> {
    let width = design::TOAST_MAX_WIDTH.min(area.width.saturating_sub(design::INLINE_GAP));
    if width < design::TOAST_MIN_WIDTH || area.height < design::TOAST_MIN_HEIGHT {
        return None;
    }
    Some(Rect::new(
        area.right()
            .saturating_sub(design::BORDER_WIDTH)
            .saturating_sub(width),
        area.y.saturating_add(design::BORDER_WIDTH),
        width,
        design::TOAST_MIN_HEIGHT,
    ))
}

pub fn render_toasts(frame: &mut Frame, toasts: &[Toast], content_area: Rect) {
    for (toast, area) in toasts.iter().zip(toast_areas(toasts, content_area)) {
        let text_color = match toast.kind {
            ToastKind::Success => theme::SUCCESS,
            ToastKind::Info => theme::INFORMATION,
            ToastKind::Error => theme::DANGER,
        };
        frame.render_widget(Clear, area);
        let title = terminal_safe_text(&toast.title);
        let text = toast.detail.as_ref().map_or_else(
            || title.clone(),
            |detail| format!("{title}\n{}", terminal_safe_text(detail)),
        );
        frame.render_widget(
            Paragraph::new(text)
                .wrap(ratatui::widgets::Wrap { trim: true })
                .style(Style::default().fg(text_color))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme::CHROME))
                        .title(
                            Line::styled(interaction::DISMISS, enabled_control_style())
                                .alignment(Alignment::Right),
                        ),
                ),
            area,
        );
    }
}

fn toast_areas(toasts: &[Toast], area: Rect) -> Vec<Rect> {
    let width = design::TOAST_MAX_WIDTH.min(area.width.saturating_sub(design::INLINE_GAP));
    let right = area.right().saturating_sub(design::BORDER_WIDTH);
    let mut bottom = area.bottom().saturating_sub(design::INLINE_GAP);
    toasts
        .iter()
        .filter_map(|toast| {
            let inner_width = usize::from(width.saturating_sub(design::INLINE_GAP))
                .max(usize::from(design::SINGLE_LINE_HEIGHT));
            let text_rows = std::iter::once(toast.title.as_str())
                .chain(toast.detail.as_deref())
                .map(|text| {
                    terminal_safe_text(text)
                        .chars()
                        .count()
                        .div_ceil(inner_width)
                })
                .sum::<usize>();
            let height = u16::try_from(text_rows)
                .unwrap_or(u16::MAX)
                .saturating_add(design::INLINE_GAP)
                .clamp(design::TOAST_MIN_HEIGHT, design::TOAST_MAX_HEIGHT);
            if width < design::TOAST_MIN_WIDTH || bottom < area.y.saturating_add(height) {
                return None;
            }
            let rect = Rect::new(right.saturating_sub(width), bottom - height, width, height);
            bottom = rect.y;
            Some(rect)
        })
        .collect()
}

#[must_use]
pub fn toast_at_position(toasts: &[Toast], area: Rect, column: u16, row: u16) -> Option<u64> {
    toasts
        .iter()
        .zip(toast_areas(toasts, area))
        .find_map(|(toast, area)| area.contains((column, row).into()).then_some(toast.id))
}

pub(crate) fn render_commit_editor(frame: &mut Frame, model: &Model, content_area: Rect) {
    let (area, input, commit, cancel, footer) = commit_editor_layout(content_area);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::CHROME))
            .title(" Commit message "),
        area,
    );

    let empty = model.commit_message.is_empty();
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::CHROME));
    let input_inner = input_block.inner(input);
    let field_width = usize::from(input_inner.width);
    let cursor = model.commit_message_cursor();
    let (message, cursor_offset) = if empty {
        (
            model
                .suggested_commit_message()
                .unwrap_or_else(|| "Type a message…".to_owned()),
            0,
        )
    } else {
        let start = cursor.saturating_sub(field_width.saturating_sub(1));
        (
            model
                .commit_message
                .chars()
                .skip(start)
                .take(field_width)
                .collect(),
            cursor.saturating_sub(start),
        )
    };
    frame.render_widget(
        Paragraph::new(message).style(if empty {
            Style::default().fg(theme::CHROME)
        } else {
            Style::default().fg(theme::TEXT)
        }),
        input_inner,
    );
    frame.render_widget(input_block, input);

    let commit_style = if model.primary_action() == crate::diff::PrimaryAction::Commit
        && model.primary_action_enabled()
    {
        enabled_control_style().bg(theme::SELECTION_BACKGROUND)
    } else {
        disabled_control_style()
    };
    frame.render_widget(
        Paragraph::new("[ Commit ]")
            .alignment(Alignment::Center)
            .style(commit_style),
        commit,
    );
    frame.render_widget(
        Paragraph::new("[ Cancel ]")
            .alignment(Alignment::Center)
            .style(enabled_control_style()),
        cancel,
    );
    frame.render_widget(
        Paragraph::new("Enter: commit · Esc: cancel · click outside: close")
            .alignment(Alignment::Center)
            .style(enabled_control_style()),
        footer,
    );

    let cursor_offset = cursor_offset.min(usize::from(input_inner.width.saturating_sub(1)));
    let cursor_x = input_inner
        .x
        .saturating_add(u16::try_from(cursor_offset).unwrap_or(u16::MAX));
    frame.set_cursor_position((cursor_x, input_inner.y));
}

pub(in crate::diff) fn commit_editor_layout(area: Rect) -> (Rect, Rect, Rect, Rect, Rect) {
    let width = design::COMMIT_EDITOR_WIDTH.resolve(area.width);
    let height = design::COMMIT_EDITOR_MAX_HEIGHT.min(area.height);
    let modal = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let inner = modal.inner(design::DIALOG_INSET);
    let rows = Layout::vertical([
        Constraint::Length(design::COMMIT_FIELD_HEIGHT),
        Constraint::Length(design::SINGLE_LINE_HEIGHT),
        Constraint::Length(design::SINGLE_LINE_HEIGHT),
        Constraint::Min(0),
        Constraint::Length(design::SINGLE_LINE_HEIGHT),
    ])
    .split(inner);
    let buttons = Layout::horizontal([
        Constraint::Percentage(design::EQUAL_SPLIT_PERCENT),
        Constraint::Percentage(design::EQUAL_SPLIT_PERCENT),
    ])
    .split(rows[2]);
    (modal, rows[0], buttons[0], buttons[1], rows[4])
}

pub(crate) fn commit_editor_action_at_position(
    model: &Model,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<crate::diff::Message> {
    let (dialog_area, _input, commit, cancel, _footer) = commit_editor_layout(area);
    let position = (column, row).into();
    if !dialog_area.contains(position) {
        return Some(crate::diff::Message::BlurCommitInput);
    }
    if cancel.contains(position) {
        return Some(crate::diff::Message::BlurCommitInput);
    }
    if commit.contains(position)
        && model.primary_action() == crate::diff::PrimaryAction::Commit
        && model.primary_action_enabled()
    {
        return Some(crate::diff::Message::ExecutePrimaryAction);
    }
    None
}
