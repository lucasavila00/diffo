use super::{
    Alignment, Block, Borders, Cell, Clear, Constraint, Frame, Layout, Model, Modifier, Paragraph,
    Rect, Row, Style, Table, Toast, ToastKind, input,
};
use diffo_ui::{design, theme};

pub(super) fn render_help(frame: &mut Frame, model: &Model, content_area: Rect) {
    if !model.help_open {
        return;
    }
    let area = help_layout(content_area);
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
    let rows = std::iter::once(("Tab".to_owned(), "Next activity"))
        .chain(input::help_rows())
        .map(|(keys, description)| {
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
        Row::new(["Shortcut", "Action"])
            .style(
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD),
            )
            .bottom_margin(design::SINGLE_LINE_HEIGHT),
    )
    .column_spacing(design::HELP_COLUMN_GAP);
    frame.render_widget(table, sections[0]);
    frame.render_widget(
        Paragraph::new("Esc: close").style(Style::default().fg(theme::CHROME)),
        sections[1],
    );
}

pub fn render_toasts(frame: &mut Frame, toasts: &[Toast], content_area: Rect) {
    for (toast, area) in toasts.iter().zip(toast_areas(toasts, content_area)) {
        let text_color = match toast.kind {
            ToastKind::Success => theme::SUCCESS,
            ToastKind::Info => theme::INFORMATION,
            ToastKind::Error => theme::DANGER,
        };
        frame.render_widget(Clear, area);
        let text = toast.detail.as_ref().map_or_else(
            || toast.title.clone(),
            |detail| format!("{}\n{detail}", toast.title),
        );
        frame.render_widget(
            Paragraph::new(text)
                .wrap(ratatui::widgets::Wrap { trim: true })
                .style(Style::default().fg(text_color))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme::CHROME)),
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
                .map(|text| text.chars().count().div_ceil(inner_width))
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

pub(super) fn render_commit_editor(frame: &mut Frame, model: &Model, content_area: Rect) {
    if !model.commit_input_focused() {
        return;
    }
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

    let commit_style = if model.primary_action() == diffo_app::PrimaryAction::Commit
        && model.primary_action_enabled()
    {
        Style::default()
            .bg(theme::SELECTION_BACKGROUND)
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::CHROME)
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
            .style(Style::default().fg(theme::TEXT)),
        cancel,
    );
    frame.render_widget(
        Paragraph::new("Enter: commit · Esc: cancel · click outside: close")
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

pub(super) fn commit_editor_layout(area: Rect) -> (Rect, Rect, Rect, Rect, Rect) {
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
) -> Option<diffo_app::Message> {
    let (dialog_area, _input, commit, cancel, _footer) = commit_editor_layout(area);
    let position = (column, row).into();
    if !dialog_area.contains(position) {
        return Some(diffo_app::Message::BlurCommitInput);
    }
    if cancel.contains(position) {
        return Some(diffo_app::Message::BlurCommitInput);
    }
    if commit.contains(position)
        && model.primary_action() == diffo_app::PrimaryAction::Commit
        && model.primary_action_enabled()
    {
        return Some(diffo_app::Message::ExecutePrimaryAction);
    }
    None
}

pub(super) fn help_layout(area: Rect) -> Rect {
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
