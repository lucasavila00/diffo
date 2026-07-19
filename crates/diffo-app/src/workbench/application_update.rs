use diffo_core::ApplicationCommandId;

use super::{CommandResult, ToastKind, Workbench};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpdateOutcome {
    Succeeded(String),
    Failed(String),
}

impl Workbench {
    pub fn offer_update(&mut self, current: &str, latest: &str) {
        let id = self.toasts.show(
            ToastKind::Info,
            format!("Diffo {latest} available · current {current} · use F1"),
        );
        self.persistent_toasts.insert(id);
    }

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
        let (kind, message) = match outcome {
            UpdateOutcome::Succeeded(message) => (ToastKind::Success, message),
            UpdateOutcome::Failed(message) => (ToastKind::Error, message),
        };
        let id = self.toasts.show(kind, message);
        self.persistent_toasts.insert(id);
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
        for activity in [Activity::Diff, Activity::Explorer, Activity::Search] {
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
    fn passive_update_notice_is_persistent_and_never_takes_focus() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.active = Activity::Explorer;
        workbench.set_modal(Modal::Help);

        workbench.offer_update("0.1.0", "0.2.0");
        workbench.tick(Instant::now() + Duration::from_secs(60));

        assert_eq!(workbench.active, Activity::Explorer);
        assert!(matches!(workbench.modal, Some(Modal::Help)));
        let toast = workbench.toasts.as_slice().first().unwrap();
        assert_eq!(toast.kind, ToastKind::Info);
        assert!(toast.title.contains("0.1.0"));
        assert!(toast.title.contains("0.2.0"));
        assert!(toast.title.contains("use F1"));
    }

    #[test]
    fn update_results_remain_visible_until_dismissed() {
        for (outcome, kind, text) in [
            (
                UpdateOutcome::Succeeded("Updated; quit and relaunch".to_owned()),
                ToastKind::Success,
                "quit and relaunch",
            ),
            (
                UpdateOutcome::Failed("Update verification failed".to_owned()),
                ToastKind::Error,
                "verification",
            ),
        ] {
            let mut workbench = Workbench::new(RepositorySnapshot::default());
            let id = workbench.commands.enqueue_update();
            let _ = workbench.take_application_command(Instant::now());

            workbench.update_finished(id, outcome);
            workbench.tick(Instant::now() + Duration::from_secs(60));

            let toast = workbench.toasts.as_slice().first().unwrap();
            assert_eq!(toast.kind, kind);
            assert!(toast.title.contains(text));
        }
    }
}
