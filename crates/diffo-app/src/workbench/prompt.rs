use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_core::{ApplicationCommandId, GitPrompt, PromptId};
use diffo_ui::{design, enabled_control_style, terminal_safe_text, theme};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::{CommandState, PromptResponse, Workbench, WorkbenchEffect};

pub(super) struct PromptModal {
    pub(super) command_id: ApplicationCommandId,
    pub(super) id: PromptId,
    pub(super) prompt: GitPrompt,
    input: String,
    pub(super) confirm_choice: ConfirmChoice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConfirmChoice {
    Cancel,
    Continue,
}

impl Workbench {
    pub fn open_prompt(
        &mut self,
        command_id: ApplicationCommandId,
        id: PromptId,
        prompt: GitPrompt,
    ) -> bool {
        if self.prompt.is_some()
            || !self.commands.active().is_some_and(|command| {
                command.id == command_id
                    && matches!(
                        command.state,
                        CommandState::Running | CommandState::Cancelling
                    )
            })
            || self.last_prompt_id.is_some_and(|last| id.0 <= last.0)
        {
            return false;
        }
        self.full_screen = false;
        self.full_screen_pending = false;
        self.prompt = Some(PromptModal {
            command_id,
            id,
            prompt,
            input: String::new(),
            confirm_choice: ConfirmChoice::Cancel,
        });
        true
    }

    pub(super) fn close_prompt(&mut self, command_id: ApplicationCommandId) {
        if self
            .prompt
            .as_ref()
            .is_some_and(|prompt| prompt.command_id == command_id)
        {
            self.prompt = None;
        }
        self.last_prompt_id = None;
    }

    pub(super) fn handle_prompt_event(
        &mut self,
        event: &Event,
        area: Rect,
    ) -> Option<WorkbenchEffect> {
        let modal = self.prompt.as_mut()?;
        let response = match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Esc => Some(PromptResponse::Cancel),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.should_quit = true;
                    Some(PromptResponse::Cancel)
                }
                KeyCode::Enter => match modal.prompt {
                    GitPrompt::ConfirmSshHost { .. } => Some(match modal.confirm_choice {
                        ConfirmChoice::Cancel => PromptResponse::Cancel,
                        ConfirmChoice::Continue => PromptResponse::Confirm,
                    }),
                    GitPrompt::Username { .. } | GitPrompt::Secret { .. }
                        if !modal.input.is_empty() =>
                    {
                        Some(PromptResponse::Text(std::mem::take(&mut modal.input)))
                    }
                    GitPrompt::Username { .. } | GitPrompt::Secret { .. } => None,
                },
                KeyCode::Left | KeyCode::Up
                    if matches!(modal.prompt, GitPrompt::ConfirmSshHost { .. }) =>
                {
                    modal.confirm_choice = ConfirmChoice::Cancel;
                    None
                }
                KeyCode::Right | KeyCode::Down
                    if matches!(modal.prompt, GitPrompt::ConfirmSshHost { .. }) =>
                {
                    modal.confirm_choice = ConfirmChoice::Continue;
                    None
                }
                KeyCode::Backspace if !matches!(modal.prompt, GitPrompt::ConfirmSshHost { .. }) => {
                    modal.input.pop();
                    None
                }
                KeyCode::Char(character)
                    if !matches!(modal.prompt, GitPrompt::ConfirmSshHost { .. })
                        && !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && !character.is_control() =>
                {
                    modal.input.push(character);
                    None
                }
                _ => None,
            },
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                let layout = prompt_layout(area);
                let position = (mouse.column, mouse.row).into();
                if layout.cancel.contains(position) {
                    Some(PromptResponse::Cancel)
                } else if layout.continue_button.contains(position) {
                    match modal.prompt {
                        GitPrompt::ConfirmSshHost { .. } => Some(PromptResponse::Confirm),
                        GitPrompt::Username { .. } | GitPrompt::Secret { .. }
                            if !modal.input.is_empty() =>
                        {
                            Some(PromptResponse::Text(std::mem::take(&mut modal.input)))
                        }
                        GitPrompt::Username { .. } | GitPrompt::Secret { .. } => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        let response = response?;
        let prompt = self.prompt.take()?;
        self.last_prompt_id = Some(prompt.id);
        if matches!(response, PromptResponse::Cancel) {
            self.commands.cancel(prompt.command_id);
        }
        Some(WorkbenchEffect::Prompt {
            command_id: prompt.command_id,
            prompt_id: prompt.id,
            response,
        })
    }
}

pub(super) struct PromptLayout {
    modal: Rect,
    message: Rect,
    input: Rect,
    cancel: Rect,
    pub(super) continue_button: Rect,
    footer: Rect,
}

pub(super) fn prompt_layout(area: Rect) -> PromptLayout {
    let width = design::COMMIT_EDITOR_WIDTH.resolve(area.width);
    let height = design::COMMIT_EDITOR_MAX_HEIGHT.min(area.height);
    let modal = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let rows = Layout::vertical([
        Constraint::Length(design::PROMPT_MESSAGE_HEIGHT),
        Constraint::Length(design::COMMIT_FIELD_HEIGHT),
        Constraint::Length(design::SINGLE_LINE_HEIGHT),
        Constraint::Min(0),
        Constraint::Length(design::SINGLE_LINE_HEIGHT),
    ])
    .split(modal.inner(design::DIALOG_INSET));
    let buttons = Layout::horizontal([
        Constraint::Percentage(design::EQUAL_SPLIT_PERCENT),
        Constraint::Percentage(design::EQUAL_SPLIT_PERCENT),
    ])
    .split(rows[2]);
    PromptLayout {
        modal,
        message: rows[0],
        input: rows[1],
        cancel: buttons[0],
        continue_button: buttons[1],
        footer: rows[4],
    }
}

pub(super) fn render_prompt(frame: &mut Frame, modal: &PromptModal, area: Rect) {
    let layout = prompt_layout(area);
    frame.render_widget(Clear, layout.modal);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::CHROME))
            .title(" Git prompt "),
        layout.modal,
    );
    let (message, secret) = match &modal.prompt {
        GitPrompt::Username { host } => {
            (format!("Username for {}", terminal_safe_text(host)), false)
        }
        GitPrompt::Secret { kind, context } => {
            let label = match kind {
                diffo_core::SecretKind::HttpsSecret => "Secret for",
                diffo_core::SecretKind::SshKeyPassphrase => "Passphrase for",
            };
            (format!("{label} {}", terminal_safe_text(context)), true)
        }
        GitPrompt::ConfirmSshHost { host, fingerprint } => (
            format!(
                "Trust {}?\n{}",
                terminal_safe_text(host),
                terminal_safe_text(fingerprint)
            ),
            false,
        ),
    };
    frame.render_widget(Paragraph::new(message), layout.message);
    if !matches!(modal.prompt, GitPrompt::ConfirmSshHost { .. }) {
        let field = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::CHROME));
        let inner = field.inner(layout.input);
        let value = if secret {
            "•".repeat(modal.input.chars().count())
        } else {
            terminal_safe_text(&modal.input)
        };
        let width = usize::from(inner.width);
        let visible = value
            .chars()
            .rev()
            .take(width)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        let cursor = u16::try_from(visible.chars().count()).unwrap_or(u16::MAX);
        frame.render_widget(Paragraph::new(visible), inner);
        frame.render_widget(field, layout.input);
        frame.set_cursor_position((inner.x.saturating_add(cursor.min(inner.width)), inner.y));
    }
    let confirm = matches!(modal.prompt, GitPrompt::ConfirmSshHost { .. });
    let cancel_selected = confirm && modal.confirm_choice == ConfirmChoice::Cancel;
    let continue_selected = confirm && modal.confirm_choice == ConfirmChoice::Continue;
    frame.render_widget(
        Paragraph::new("[ Cancel ]")
            .alignment(Alignment::Center)
            .style(prompt_button_style(cancel_selected, true)),
        layout.cancel,
    );
    frame.render_widget(
        Paragraph::new("[ Continue ]")
            .alignment(Alignment::Center)
            .style(prompt_button_style(
                continue_selected,
                confirm || !modal.input.is_empty(),
            )),
        layout.continue_button,
    );
    frame.render_widget(
        Paragraph::new(if confirm {
            "Arrows: select · Enter: choose · Esc: cancel"
        } else {
            "Enter: continue · Esc: cancel"
        })
        .alignment(Alignment::Center)
        .style(enabled_control_style()),
        layout.footer,
    );
}

fn prompt_button_style(selected: bool, enabled: bool) -> Style {
    let mut style = if enabled {
        enabled_control_style()
    } else {
        Style::default().fg(theme::CHROME)
    };
    if selected {
        style = style
            .bg(theme::SELECTION_BACKGROUND)
            .add_modifier(Modifier::BOLD);
    }
    style
}
