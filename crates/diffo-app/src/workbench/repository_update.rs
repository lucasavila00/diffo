use std::time::{Duration, Instant};

use diffo_core::{
    ApplicationCommandId, OperationFailure, OperationResult, RepositoryAction, RepositorySnapshot,
    RepositoryUpdate, RepositoryUpdateKind, SyncProgress,
};

use super::{
    ApplicationAction, CommandProgressState, CommandResult, Message, ToastKind, Workbench,
};

impl Workbench {
    pub fn accept_sync_progress(
        &mut self,
        command_id: ApplicationCommandId,
        progress: SyncProgress,
    ) {
        let Some(command) = self.commands.active_mut().filter(|command| {
            command.id == command_id
                && command.action == ApplicationAction::Repository(RepositoryAction::Sync)
        }) else {
            return;
        };
        command.label = crate::diff::model::sync_progress_label(&progress);
        if let SyncProgress::Plan(plan) = progress {
            self.show_toast(ToastKind::Info, crate::diff::model::sync_plan_title(&plan));
        }
    }

    #[must_use]
    pub const fn repository_generation(&self) -> u64 {
        self.repository_generation
    }

    pub fn accept_repository_update(&mut self, update: RepositoryUpdate) -> bool {
        if update.generation <= self.repository_generation {
            return false;
        }
        self.repository_generation = update.generation;
        match update.kind {
            RepositoryUpdateKind::Snapshot(snapshot) => self.repository_changed(snapshot),
            RepositoryUpdateKind::RefreshFailed(message) => self.operation_failed(message),
            RepositoryUpdateKind::CommandCompleted {
                command_id,
                action,
                result,
                snapshot,
            } => {
                if self.active_command_id() == Some(command_id) {
                    self.operation_completed(command_id, action, result, snapshot);
                } else {
                    self.repository_changed(snapshot);
                }
            }
            RepositoryUpdateKind::CommandFailed {
                command_id,
                failure,
                snapshot,
            } => self.action_failed_with_snapshot(command_id, failure, snapshot),
            RepositoryUpdateKind::CommandCancelled {
                command_id,
                action,
                snapshot,
            } => {
                self.operation_cancelled(command_id, action, snapshot);
            }
        }
        true
    }

    pub fn operation_failed(&mut self, message: String) {
        let _ = self.update_diff(Message::OperationFailed(message));
    }

    pub fn operation_completed(
        &mut self,
        id: ApplicationCommandId,
        action: RepositoryAction,
        result: OperationResult,
        snapshot: RepositorySnapshot,
    ) {
        if self
            .commands
            .acknowledge(id, CommandResult::Succeeded)
            .is_none()
        {
            return;
        }
        self.close_prompt(id);
        self.finish_command_progress(id);
        let _ = self.update_diff(Message::OperationCompleted(action, result, snapshot));
    }

    pub fn action_failed(&mut self, id: ApplicationCommandId, failure: OperationFailure) {
        self.action_failed_with_snapshot(id, failure, None);
    }

    fn action_failed_with_snapshot(
        &mut self,
        id: ApplicationCommandId,
        failure: OperationFailure,
        snapshot: Option<RepositorySnapshot>,
    ) {
        if self.handle_delete_branch_failure(id, &failure) {
            return;
        }
        if self
            .commands
            .acknowledge(id, CommandResult::Failed)
            .is_none()
        {
            return;
        }
        self.close_prompt(id);
        self.finish_command_progress(id);
        let _ = self.update_diff(Message::ActionFailed(failure));
        if let Some(snapshot) = snapshot {
            self.repository_changed(snapshot);
        }
    }

    pub fn operation_cancelled(
        &mut self,
        id: ApplicationCommandId,
        action: RepositoryAction,
        snapshot: RepositorySnapshot,
    ) {
        if self
            .commands
            .acknowledge(id, CommandResult::Cancelled)
            .is_none()
        {
            return;
        }
        self.close_prompt(id);
        self.finish_command_progress(id);
        let _ = self.update_diff(Message::OperationCancelled(action));
        self.repository_changed(snapshot);
    }

    pub(super) fn finish_command_progress(&mut self, id: ApplicationCommandId) {
        if self.command_progress.command_id() == Some(id) {
            self.command_progress = CommandProgressState::Hidden;
            self.command_animation_tick = 0;
        }
    }

    pub(super) fn expire_toasts(&mut self, now: Instant) {
        self.toast_deadlines
            .retain(|id, _| self.toasts.as_slice().iter().any(|toast| toast.id == *id));
        for toast in self.toasts.as_slice() {
            if toast.kind != ToastKind::Error && !self.persistent_toasts.contains(&toast.id) {
                self.toast_deadlines
                    .entry(toast.id)
                    .or_insert_with(|| now + Duration::from_secs(3));
            }
        }
        let expired = self
            .toast_deadlines
            .iter()
            .filter_map(|(id, deadline)| (*deadline <= now).then_some(*id))
            .collect::<Vec<_>>();
        for id in expired {
            self.toasts.dismiss(id);
            self.toast_deadlines.remove(&id);
        }
        self.persistent_toasts
            .retain(|id| self.toasts.as_slice().iter().any(|toast| toast.id == *id));
    }
}
