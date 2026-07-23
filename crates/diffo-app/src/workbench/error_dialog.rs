use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_ui::{design, icons, modal_block, mouse_target_style, terminal_safe_text, theme};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::{Clear, Paragraph, Wrap},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ErrorDialog {
    pub(super) title: String,
    pub(super) detail: String,
}

impl ErrorDialog {
    pub(super) fn new(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
        }
    }

    pub(super) fn render(&self, frame: &mut Frame, area: Rect) {
        let layout = error_dialog_layout(area);
        frame.render_widget(Clear, layout.modal);
        frame.render_widget(
            modal_block(&self.title).title(
                Line::styled(icons::DISMISS, mouse_target_style()).alignment(Alignment::Right),
            ),
            layout.modal,
        );
        frame.render_widget(
            Paragraph::new(terminal_safe_text(&self.detail))
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(theme::DANGER)),
            layout.message,
        );
        frame.render_widget(
            Paragraph::new("[ OK ]")
                .alignment(Alignment::Center)
                .style(mouse_target_style()),
            layout.ok,
        );
        frame.render_widget(
            Paragraph::new("Enter / Esc: close")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme::CHROME)),
            layout.footer,
        );
    }

    pub(super) fn handle_event(event: &Event, area: Rect) -> ErrorDialogEvent {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Enter | KeyCode::Esc if key.modifiers == KeyModifiers::NONE => {
                    ErrorDialogEvent::Dismiss
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    ErrorDialogEvent::Quit
                }
                _ => ErrorDialogEvent::Consumed,
            },
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                let layout = error_dialog_layout(area);
                let position = (mouse.column, mouse.row).into();
                if layout.ok.contains(position)
                    || (mouse.row == layout.modal.y
                        && mouse.column == layout.modal.right().saturating_sub(2))
                {
                    ErrorDialogEvent::Dismiss
                } else {
                    ErrorDialogEvent::Consumed
                }
            }
            _ => ErrorDialogEvent::Consumed,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ErrorDialogEvent {
    Consumed,
    Dismiss,
    Quit,
}

pub(super) struct ErrorDialogLayout {
    pub(super) modal: Rect,
    message: Rect,
    pub(super) ok: Rect,
    footer: Rect,
}

pub(super) fn error_dialog_layout(area: Rect) -> ErrorDialogLayout {
    let width = design::COMMIT_EDITOR_WIDTH.resolve(area.width);
    let height = design::COMMIT_EDITOR_MAX_HEIGHT.min(area.height);
    let modal = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let rows = Layout::vertical([
        Constraint::Min(design::PROMPT_MESSAGE_HEIGHT),
        Constraint::Length(design::SINGLE_LINE_HEIGHT),
        Constraint::Length(design::SINGLE_LINE_HEIGHT),
    ])
    .split(modal.inner(design::DIALOG_INSET));
    ErrorDialogLayout {
        modal,
        message: rows[0],
        ok: rows[1],
        footer: rows[2],
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyEvent, MouseEvent};
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    fn click(column: u16, row: u16) -> Event {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn enter_escape_ok_and_close_button_dismiss_but_outside_click_does_not() {
        let area = Rect::new(0, 0, 100, 30);
        let layout = error_dialog_layout(area);
        for event in [
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            click(layout.ok.x, layout.ok.y),
            click(layout.modal.right().saturating_sub(2), layout.modal.y),
        ] {
            assert_eq!(
                ErrorDialog::handle_event(&event, area),
                ErrorDialogEvent::Dismiss
            );
        }
        assert_eq!(
            ErrorDialog::handle_event(&click(0, 0), area),
            ErrorDialogEvent::Consumed
        );
    }

    #[test]
    fn rendering_makes_control_characters_inert() {
        let area = Rect::new(0, 0, 100, 30);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        let dialog = ErrorDialog::new("Push\nfailed", "detail\nnext\u{1b}[31m");

        terminal
            .draw(|frame| dialog.render(frame, frame.area()))
            .unwrap();

        let screen = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(!screen.chars().any(char::is_control));
        assert!(screen.contains("Push␊failed"));
        assert!(screen.contains("detail␊next␛[31m"));
        assert!(screen.contains("[ OK ]"));
    }
}
