use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_core::{
    ApplicationCommandId, DeleteBranchTarget, FailureKind, OperationFailure, RepositoryAction,
};
use diffo_ui::command_palette::{Command, CommandId};
use diffo_ui::{modal_block, terminal_safe_text, theme};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    widgets::{Clear, Paragraph},
};

use super::{
    CommandResult, ConfirmChoice, Message, Modal, Workbench, prompt_button_style, prompt_layout,
};

pub(super) const DELETE_BRANCH_COMMAND: CommandId = CommandId::new("git.delete_branch");
pub(super) const DELETE_BRANCH_PALETTE_COMMAND: Command = Command {
    id: DELETE_BRANCH_COMMAND,
    label: "Git: Delete Branch...",
};

pub(super) struct DeleteBranchConfirmation {
    target: DeleteBranchTarget,
    choice: ConfirmChoice,
}

impl DeleteBranchConfirmation {
    pub(super) fn new(target: DeleteBranchTarget) -> Self {
        Self {
            target,
            choice: ConfirmChoice::Cancel,
        }
    }

    pub(super) fn render(&self, frame: &mut Frame, area: Rect) {
        let layout = prompt_layout(area, true);
        frame.render_widget(Clear, layout.modal);
        frame.render_widget(modal_block("Delete branch?"), layout.modal);
        frame.render_widget(
            Paragraph::new(format!(
                "The branch \"{}\" is not fully merged. Delete anyway?",
                terminal_safe_text(&self.target.name)
            )),
            layout.message,
        );
        frame.render_widget(
            Paragraph::new("[ Cancel ]")
                .alignment(Alignment::Center)
                .style(prompt_button_style(
                    self.choice == ConfirmChoice::Cancel,
                    true,
                )),
            layout.cancel,
        );
        frame.render_widget(
            Paragraph::new("[ Delete branch ]")
                .alignment(Alignment::Center)
                .style(prompt_button_style(
                    self.choice == ConfirmChoice::Continue,
                    true,
                )),
            layout.continue_button,
        );
        frame.render_widget(
            Paragraph::new("Arrows: select · Enter: choose · Esc: cancel")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme::CHROME)),
            layout.footer,
        );
    }

    fn handle_event(&mut self, event: &Event, area: Rect) -> DeleteBranchConfirmationEvent {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Esc => DeleteBranchConfirmationEvent::Cancel,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    DeleteBranchConfirmationEvent::Quit
                }
                KeyCode::Enter => match self.choice {
                    ConfirmChoice::Cancel => DeleteBranchConfirmationEvent::Cancel,
                    ConfirmChoice::Continue => {
                        let mut target = self.target.clone();
                        target.force = true;
                        DeleteBranchConfirmationEvent::Delete(target)
                    }
                },
                KeyCode::Left | KeyCode::Up => {
                    self.choice = ConfirmChoice::Cancel;
                    DeleteBranchConfirmationEvent::Consumed
                }
                KeyCode::Right | KeyCode::Down => {
                    self.choice = ConfirmChoice::Continue;
                    DeleteBranchConfirmationEvent::Consumed
                }
                _ => DeleteBranchConfirmationEvent::Consumed,
            },
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                let layout = prompt_layout(area, true);
                let position = (mouse.column, mouse.row).into();
                if layout.cancel.contains(position) {
                    DeleteBranchConfirmationEvent::Cancel
                } else if layout.continue_button.contains(position) {
                    let mut target = self.target.clone();
                    target.force = true;
                    DeleteBranchConfirmationEvent::Delete(target)
                } else {
                    DeleteBranchConfirmationEvent::Consumed
                }
            }
            _ => DeleteBranchConfirmationEvent::Consumed,
        }
    }
}

enum DeleteBranchConfirmationEvent {
    Consumed,
    Cancel,
    Delete(DeleteBranchTarget),
    Quit,
}

impl Workbench {
    pub(super) fn handle_delete_branch_failure(
        &mut self,
        id: ApplicationCommandId,
        failure: &OperationFailure,
    ) -> bool {
        let RepositoryAction::DeleteBranch(target) = &failure.action else {
            return false;
        };
        if failure.kind != FailureKind::BranchNotFullyMerged || target.force {
            return false;
        }
        if self
            .commands
            .acknowledge(id, CommandResult::Failed)
            .is_none()
        {
            return false;
        }
        self.close_prompt(id);
        self.finish_command_progress(id);
        self.diff.model.finish_ai_commit();
        let _ = self.update_diff(Message::OperationCancelled(failure.action.clone()));
        self.set_modal(Modal::DeleteBranchConfirmation(
            DeleteBranchConfirmation::new((**target).clone()),
        ));
        true
    }

    pub(super) fn handle_delete_branch_confirmation_event(&mut self, event: &Event, area: Rect) {
        let modal_event = match self.modal.as_mut() {
            Some(Modal::DeleteBranchConfirmation(modal)) => modal.handle_event(event, area),
            _ => return,
        };
        match modal_event {
            DeleteBranchConfirmationEvent::Consumed => {}
            DeleteBranchConfirmationEvent::Cancel => self.close_modal(),
            DeleteBranchConfirmationEvent::Delete(target) => {
                self.close_modal();
                self.commands
                    .enqueue(RepositoryAction::DeleteBranch(Box::new(target)));
            }
            DeleteBranchConfirmationEvent::Quit => {
                self.should_quit = true;
                self.close_modal();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use crossterm::event::KeyEvent;
    use diffo_core::{OperationFailure, RepositorySnapshot};

    use super::*;

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn target(force: bool) -> DeleteBranchTarget {
        DeleteBranchTarget {
            name: "topic".to_owned(),
            full_ref: "refs/heads/topic".to_owned(),
            object_id: "abc".to_owned(),
            force,
        }
    }

    fn fail_safe_delete(workbench: &mut Workbench) {
        let action = RepositoryAction::DeleteBranch(Box::new(target(false)));
        let id = workbench.commands.enqueue(action.clone());
        let _ = workbench
            .take_application_command(Instant::now())
            .expect("delete command starts");
        workbench.action_failed(
            id,
            OperationFailure {
                action,
                kind: FailureKind::BranchNotFullyMerged,
                detail: "branch is not fully merged".to_owned(),
            },
        );
    }

    #[test]
    fn unmerged_failure_opens_cancel_first_confirmation_without_a_toast() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        fail_safe_delete(&mut workbench);

        assert!(matches!(
            workbench.modal,
            Some(Modal::DeleteBranchConfirmation(DeleteBranchConfirmation {
                choice: ConfirmChoice::Cancel,
                ..
            }))
        ));
        assert!(workbench.toasts.as_slice().is_empty());
        assert!(workbench.commands.active().is_none());

        workbench.handle_delete_branch_confirmation_event(&key(KeyCode::Enter), Rect::default());
        assert!(workbench.modal.is_none());
        assert_eq!(workbench.commands.queued_len(), 0);
    }

    #[test]
    fn confirming_unmerged_failure_queues_forced_delete_of_same_ref() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        fail_safe_delete(&mut workbench);
        let area = Rect::new(0, 0, 100, 30);

        workbench.handle_delete_branch_confirmation_event(&key(KeyCode::Right), area);
        workbench.handle_delete_branch_confirmation_event(&key(KeyCode::Enter), area);

        let command = workbench
            .take_application_command(Instant::now())
            .expect("forced delete starts");
        assert!(matches!(
            command.action,
            super::super::ApplicationAction::Repository(RepositoryAction::DeleteBranch(selected))
                if *selected == DeleteBranchTarget {
                    force: true,
                    ..target(false)
                }
        ));
    }

    #[test]
    fn other_delete_failures_use_the_error_dialog() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let action = RepositoryAction::DeleteBranch(Box::new(target(false)));
        let id = workbench.commands.enqueue(action.clone());
        let _ = workbench.take_application_command(Instant::now());

        workbench.action_failed(
            id,
            OperationFailure {
                action,
                kind: FailureKind::RefChanged,
                detail: "selected branch changed".to_owned(),
            },
        );

        assert!(matches!(
            workbench.modal,
            Some(Modal::Error(ref error))
                if error.title == "Delete branch failed"
                    && error.detail == "selected branch changed"
        ));
        assert!(workbench.toasts.as_slice().is_empty());
    }
}
