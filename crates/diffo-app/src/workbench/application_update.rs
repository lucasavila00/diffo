use diffo_core::ApplicationCommandId;

use super::{CommandResult, ToastKind, Workbench};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpdateOutcome {
    Succeeded(String),
    Failed(String),
}

impl Workbench {
    pub fn update_finished(&mut self, id: ApplicationCommandId, outcome: UpdateOutcome) {
        let result = if matches!(outcome, UpdateOutcome::Succeeded(_)) {
            CommandResult::Succeeded
        } else {
            CommandResult::Failed
        };
        if self.commands.acknowledge(id, result).is_none() {
            return;
        }
        self.finish_command_progress(id);
        match outcome {
            UpdateOutcome::Succeeded(message) => self.show_toast(ToastKind::Success, message),
            UpdateOutcome::Failed(message) => self.show_error("Update failed", message),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use diffo_core::RepositorySnapshot;

    use super::*;
    use crate::workbench::{Activity, ApplicationAction, Modal, UPDATE_COMMAND};

    #[test]
    fn update_command_is_available_in_every_activity_and_uses_the_shared_queue() {
        for activity in [Activity::Diff, Activity::Explorer, Activity::Review] {
            let mut workbench = Workbench::new(RepositorySnapshot::default());
            workbench.active = activity;
            workbench.open_active_palette();
            let Some(Modal::CommandPalette(palette)) = workbench.modal.as_ref() else {
                panic!("command palette should be open");
            };
            assert!(palette.matches().iter().any(|command| {
                command.id == UPDATE_COMMAND && command.label == "Application: Update Diffo"
            }));
            workbench.close_modal();

            let _ = workbench.execute_palette_command(UPDATE_COMMAND);
            let command = workbench
                .take_application_command(Instant::now())
                .expect("update command queued");
            assert_eq!(command.action, ApplicationAction::Update);
        }
    }

    #[test]
    fn update_success_expires_and_failure_opens_the_error_dialog() {
        let mut success = Workbench::new(RepositorySnapshot::default());
        let id = success.commands.enqueue_update();
        let now = Instant::now();
        let _ = success.take_application_command(now);
        success.update_finished(
            id,
            UpdateOutcome::Succeeded("Updated; quit and relaunch".to_owned()),
        );
        assert_eq!(success.toasts.as_slice()[0].kind, ToastKind::Success);
        success.tick(now);
        success.tick(now + Duration::from_mins(1));
        assert!(success.toasts.as_slice().is_empty());

        let mut failure = Workbench::new(RepositorySnapshot::default());
        let id = failure.commands.enqueue_update();
        let _ = failure.take_application_command(now);
        failure.update_finished(
            id,
            UpdateOutcome::Failed("Update verification failed".to_owned()),
        );
        assert!(matches!(
            failure.modal,
            Some(Modal::Error(ref error))
                if error.title == "Update failed" && error.detail.contains("verification")
        ));
        assert!(failure.toasts.as_slice().is_empty());
    }
}
