use crate::diff::{
    Alignment, Block, Borders, Clear, Constraint, Frame, Layout, Line, Model, Paragraph, Rect,
    Style, Toast, ToastKind, terminal_safe_text,
};
use diffo_core::ApplicationCommandId;
use diffo_ui::{
    command_progress_style, design, disabled_control_style, icons, mouse_target_style, theme,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandProgressState {
    Active,
    Cancelling,
    Queued,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandProgressRow {
    pub id: ApplicationCommandId,
    pub label: String,
    pub state: CommandProgressState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandProgressAction {
    Cancel(ApplicationCommandId),
    CancelAll,
}

pub struct CommandProgress<'a> {
    pub rows: &'a [CommandProgressRow],
    pub hidden: usize,
    pub animation_tick: usize,
}

pub fn render_command_progress(
    frame: &mut Frame,
    progress: CommandProgress<'_>,
    content_area: Rect,
) {
    let Some(area) = command_progress_area(content_area, progress.rows.len()) else {
        return;
    };
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::CHROME))
            .title(" Commands "),
        area,
    );
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: design::BORDER_WIDTH,
        vertical: design::BORDER_WIDTH,
    });
    for (index, row) in progress.rows.iter().enumerate() {
        let y = inner
            .y
            .saturating_add(u16::try_from(index).unwrap_or(u16::MAX));
        if y >= inner.bottom().saturating_sub(design::SINGLE_LINE_HEIGHT) {
            break;
        }
        let label = match row.state {
            CommandProgressState::Active => format!(
                "{} {}…",
                icons::SPINNER[(progress.animation_tick / 2) % icons::SPINNER.len()],
                row.label
            ),
            CommandProgressState::Cancelling => {
                format!("Cancelling {}…", row.label.to_lowercase())
            }
            CommandProgressState::Queued => format!("{}. {}", index + 1, row.label),
        };
        let row_area = Rect::new(
            inner.x,
            y,
            inner.width.saturating_sub(design::INLINE_GAP),
            design::SINGLE_LINE_HEIGHT,
        );
        frame.render_widget(
            Paragraph::new(terminal_safe_text(&label)).style(match row.state {
                CommandProgressState::Active | CommandProgressState::Cancelling => {
                    command_progress_style(progress.animation_tick)
                }
                CommandProgressState::Queued => Style::default(),
            }),
            row_area,
        );
        frame.render_widget(
            Paragraph::new(Line::styled(icons::DISMISS, mouse_target_style()))
                .alignment(Alignment::Right),
            Rect::new(
                inner.right().saturating_sub(design::SINGLE_LINE_HEIGHT),
                y,
                design::SINGLE_LINE_HEIGHT,
                design::SINGLE_LINE_HEIGHT,
            ),
        );
    }
    let footer = Rect::new(
        inner.x,
        inner.bottom().saturating_sub(design::SINGLE_LINE_HEIGHT),
        inner.width,
        design::SINGLE_LINE_HEIGHT,
    );
    let hidden = (progress.hidden > 0).then(|| format!("+{} more", progress.hidden));
    frame.render_widget(Paragraph::new(hidden.unwrap_or_default()), footer);
    frame.render_widget(
        Paragraph::new(Line::styled("× Cancel all", mouse_target_style()))
            .alignment(Alignment::Right),
        footer,
    );
}

#[must_use]
pub fn command_action_at_position(
    progress: CommandProgress<'_>,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<CommandProgressAction> {
    let progress_area = command_progress_area(area, progress.rows.len())?;
    let inner = progress_area.inner(ratatui::layout::Margin {
        horizontal: design::BORDER_WIDTH,
        vertical: design::BORDER_WIDTH,
    });
    if !inner.contains((column, row).into()) {
        return None;
    }
    if row == inner.bottom().saturating_sub(design::SINGLE_LINE_HEIGHT) {
        return (column >= inner.right().saturating_sub(12))
            .then_some(CommandProgressAction::CancelAll);
    }
    let index = usize::from(row.saturating_sub(inner.y));
    (column == inner.right().saturating_sub(design::SINGLE_LINE_HEIGHT))
        .then(|| progress.rows.get(index))
        .flatten()
        .map(|command| CommandProgressAction::Cancel(command.id))
}

fn command_progress_area(area: Rect, rows: usize) -> Option<Rect> {
    let width = design::TOAST_MAX_WIDTH.min(area.width.saturating_sub(design::INLINE_GAP));
    let height = u16::try_from(rows)
        .unwrap_or(u16::MAX)
        .saturating_add(design::TOAST_MIN_HEIGHT);
    if rows == 0 || width < design::TOAST_MIN_WIDTH || area.height < height {
        return None;
    }
    Some(Rect::new(
        area.right()
            .saturating_sub(design::BORDER_WIDTH)
            .saturating_sub(width),
        area.y.saturating_add(design::BORDER_WIDTH),
        width,
        height,
    ))
}

pub fn render_toasts(frame: &mut Frame, toasts: &[Toast], content_area: Rect) {
    for (toast, area) in toasts.iter().zip(toast_areas(toasts, content_area)) {
        let text_color = match toast.kind {
            ToastKind::Success => theme::SUCCESS,
            ToastKind::Info => theme::INFORMATION,
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
                            Line::styled(icons::DISMISS, mouse_target_style())
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

    let commit_style = if model.commit_enabled() {
        mouse_target_style()
    } else {
        disabled_control_style()
    };
    frame.render_widget(
        Paragraph::new(if model.merge_phase().is_some() {
            "[ Complete merge (Enter) ]"
        } else {
            "[ Commit (Enter) ]"
        })
        .alignment(Alignment::Center)
        .style(commit_style),
        commit,
    );
    frame.render_widget(
        Paragraph::new("[ Cancel (Esc) ]")
            .alignment(Alignment::Center)
            .style(mouse_target_style()),
        cancel,
    );
    frame.render_widget(
        Paragraph::new("Click outside to close")
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::CHROME)),
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
    if commit.contains(position) && model.commit_enabled() {
        return Some(crate::diff::Message::ExecuteCommit);
    }
    None
}
