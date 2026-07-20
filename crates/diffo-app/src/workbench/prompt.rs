use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_core::{ApplicationCommandId, GitPrompt, PromptId};
use diffo_ui::{design, modal_block, mouse_target_style, terminal_safe_text, theme};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::{CommandState, Modal, PromptResponse, Workbench, WorkbenchEffect};

pub(super) struct PromptModal {
    pub(super) command_id: ApplicationCommandId,
    pub(super) id: PromptId,
    pub(super) prompt: GitPrompt,
    input: String,
    pub(super) confirm_choice: ConfirmChoice,
}

impl PromptModal {
    fn new(command_id: ApplicationCommandId, id: PromptId, prompt: GitPrompt) -> Self {
        Self {
            command_id,
            id,
            prompt,
            input: String::new(),
            confirm_choice: ConfirmChoice::Cancel,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConfirmChoice {
    Cancel,
    Continue,
}

impl Workbench {
    #[must_use]
    pub fn protected_push_prompt_open(&self) -> bool {
        matches!(
            self.modal,
            Some(Modal::GitPrompt(ref modal))
                if matches!(modal.prompt, GitPrompt::ConfirmProtectedBranchPush { .. })
        )
    }

    pub fn open_prompt(
        &mut self,
        command_id: ApplicationCommandId,
        id: PromptId,
        prompt: GitPrompt,
    ) -> bool {
        if matches!(self.modal, Some(Modal::GitPrompt(_)))
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
        self.set_modal(Modal::GitPrompt(PromptModal::new(command_id, id, prompt)));
        true
    }

    pub(super) fn close_prompt(&mut self, command_id: ApplicationCommandId) {
        if matches!(
            self.modal,
            Some(Modal::GitPrompt(ref prompt)) if prompt.command_id == command_id
        ) {
            self.close_modal();
        }
        self.last_prompt_id = None;
    }

    pub(super) fn handle_prompt_event(
        &mut self,
        event: &Event,
        area: Rect,
    ) -> Option<WorkbenchEffect> {
        let Some(Modal::GitPrompt(modal)) = self.modal.as_mut() else {
            return None;
        };
        let response = match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Esc => Some(PromptResponse::Cancel),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.should_quit = true;
                    Some(PromptResponse::Cancel)
                }
                KeyCode::Enter => match modal.prompt {
                    GitPrompt::ConfirmSshHost { .. }
                    | GitPrompt::ConfirmProtectedBranchPush { .. } => {
                        Some(match modal.confirm_choice {
                            ConfirmChoice::Cancel => PromptResponse::Cancel,
                            ConfirmChoice::Continue => PromptResponse::Confirm,
                        })
                    }
                    GitPrompt::Username { .. } | GitPrompt::Secret { .. }
                        if !modal.input.is_empty() =>
                    {
                        Some(PromptResponse::Text(std::mem::take(&mut modal.input)))
                    }
                    GitPrompt::Username { .. } | GitPrompt::Secret { .. } => None,
                },
                KeyCode::Left | KeyCode::Up if is_confirmation(&modal.prompt) => {
                    modal.confirm_choice = ConfirmChoice::Cancel;
                    None
                }
                KeyCode::Right | KeyCode::Down if is_confirmation(&modal.prompt) => {
                    modal.confirm_choice = ConfirmChoice::Continue;
                    None
                }
                KeyCode::Backspace if !is_confirmation(&modal.prompt) => {
                    modal.input.pop();
                    None
                }
                KeyCode::Char(character)
                    if !is_confirmation(&modal.prompt)
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
                let layout = prompt_layout(area, is_confirmation(&modal.prompt));
                let position = (mouse.column, mouse.row).into();
                if layout.cancel.contains(position) {
                    Some(PromptResponse::Cancel)
                } else if layout.continue_button.contains(position) {
                    match modal.prompt {
                        GitPrompt::ConfirmSshHost { .. }
                        | GitPrompt::ConfirmProtectedBranchPush { .. } => {
                            Some(PromptResponse::Confirm)
                        }
                        GitPrompt::Username { .. } | GitPrompt::Secret { .. }
                            if !modal.input.is_empty() =>
                        {
                            Some(PromptResponse::Text(std::mem::take(&mut modal.input)))
                        }
                        GitPrompt::Username { .. } | GitPrompt::Secret { .. } => None,
                    }
                } else if matches!(modal.prompt, GitPrompt::ConfirmProtectedBranchPush { .. })
                    && !layout.modal.contains(position)
                {
                    Some(PromptResponse::Cancel)
                } else {
                    None
                }
            }
            _ => None,
        };
        let response = response?;
        let Modal::GitPrompt(prompt) = self.modal.take()? else {
            return None;
        };
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
    pub(super) modal: Rect,
    message: Rect,
    input: Rect,
    cancel: Rect,
    pub(super) continue_button: Rect,
    footer: Rect,
}

pub(super) fn prompt_layout(area: Rect, confirmation: bool) -> PromptLayout {
    let width = design::COMMIT_EDITOR_WIDTH.resolve(area.width);
    let height = design::COMMIT_EDITOR_MAX_HEIGHT.min(area.height);
    let modal = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let rows = Layout::vertical([
        Constraint::Length(if confirmation {
            design::PROMPT_MESSAGE_HEIGHT.saturating_add(design::COMMIT_FIELD_HEIGHT)
        } else {
            design::PROMPT_MESSAGE_HEIGHT
        }),
        Constraint::Length(if confirmation {
            0
        } else {
            design::COMMIT_FIELD_HEIGHT
        }),
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
    let layout = prompt_layout(area, is_confirmation(&modal.prompt));
    frame.render_widget(Clear, layout.modal);
    let title = if matches!(modal.prompt, GitPrompt::ConfirmProtectedBranchPush { .. }) {
        "Confirm push"
    } else {
        "Git prompt"
    };
    frame.render_widget(modal_block(title), layout.modal);
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
        GitPrompt::ConfirmProtectedBranchPush {
            destination,
            commits,
        } => {
            let noun = if *commits == 1 { "commit" } else { "commits" };
            (
                format!(
                    "Push {commits} {noun} directly to {}?\n\nThis bypasses the branch and pull-request workflow.",
                    terminal_safe_text(destination)
                ),
                false,
            )
        }
    };
    frame.render_widget(Paragraph::new(message), layout.message);
    if !is_confirmation(&modal.prompt) {
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
    let confirm = is_confirmation(&modal.prompt);
    let cancel_selected = confirm && modal.confirm_choice == ConfirmChoice::Cancel;
    let continue_selected = confirm && modal.confirm_choice == ConfirmChoice::Continue;
    frame.render_widget(
        Paragraph::new("[ Cancel ]")
            .alignment(Alignment::Center)
            .style(prompt_button_style(cancel_selected, true)),
        layout.cancel,
    );
    let continue_label = if matches!(modal.prompt, GitPrompt::ConfirmProtectedBranchPush { .. }) {
        "[ Push ]"
    } else {
        "[ Continue ]"
    };
    frame.render_widget(
        Paragraph::new(continue_label)
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
        .style(Style::default().fg(theme::CHROME)),
        layout.footer,
    );
}

fn is_confirmation(prompt: &GitPrompt) -> bool {
    matches!(
        prompt,
        GitPrompt::ConfirmSshHost { .. } | GitPrompt::ConfirmProtectedBranchPush { .. }
    )
}

fn prompt_button_style(selected: bool, enabled: bool) -> Style {
    let mut style = if enabled {
        mouse_target_style()
    } else {
        Style::default().fg(theme::CHROME)
    };
    if selected {
        style = style.bg(theme::SELECTION_BACKGROUND);
    }
    style
}
