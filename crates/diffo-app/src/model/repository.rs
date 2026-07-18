use super::toast::operation_result_toast;
use super::*;

impl Model {
    pub fn repository_changed(&mut self, snapshot: RepositorySnapshot) {
        self.install_snapshot(snapshot, false);
    }

    pub(super) fn install_snapshot(
        &mut self,
        snapshot: RepositorySnapshot,
        action_completed: bool,
    ) {
        let intended_selection = action_completed
            .then(|| self.selection_after_action.take())
            .flatten();
        let old_selected = self.selected.clone();
        let old_cursor = self.cursor;
        let keys = file_keys(&snapshot);

        let cursor = intended_selection
            .as_ref()
            .and_then(|selected| keys.iter().position(|key| key == selected))
            .or_else(|| {
                old_selected
                    .as_ref()
                    .and_then(|selected| keys.iter().position(|key| key == selected))
            })
            .unwrap_or_else(|| old_cursor.min(keys.len().saturating_sub(1)));
        let selected = keys.get(cursor).cloned();
        self.snapshot = snapshot;
        self.cursor = cursor;
        self.selected = selected;
        self.error = None;
    }

    pub(super) fn finish_pending_operation(&mut self) {
        if matches!(self.pending_operation, Some(RepositoryAction::Commit(_))) {
            self.commit_message.clear();
            self.commit_message_cursor = 0;
        }
        self.pending_operation = None;
    }

    pub fn complete_operation(&mut self, result: &OperationResult, snapshot: RepositorySnapshot) {
        let is_async_result = matches!(
            result,
            OperationResult::Fetch { .. }
                | OperationResult::Pull { .. }
                | OperationResult::Push { .. }
                | OperationResult::Commit { .. }
        );
        let finishes_pending = matches!(
            (self.pending_operation.as_ref(), result),
            (Some(RepositoryAction::Fetch), OperationResult::Fetch { .. })
                | (Some(RepositoryAction::Pull), OperationResult::Pull { .. })
                | (Some(RepositoryAction::Push), OperationResult::Push { .. })
                | (
                    Some(RepositoryAction::Commit(_)),
                    OperationResult::Commit { .. }
                )
        );
        if is_async_result && !finishes_pending {
            self.install_snapshot(snapshot, false);
            return;
        }
        if finishes_pending {
            self.finish_pending_operation();
        }
        self.install_snapshot(snapshot, true);
        if let Some((kind, title)) = operation_result_toast(result) {
            self.push_toast(kind, title, None);
        }
    }
}

pub(super) fn same_repository_operation(left: &RepositoryAction, right: &RepositoryAction) -> bool {
    matches!(
        (left, right),
        (RepositoryAction::Fetch, RepositoryAction::Fetch)
            | (RepositoryAction::Pull, RepositoryAction::Pull)
            | (RepositoryAction::Push, RepositoryAction::Push)
            | (RepositoryAction::Commit(_), RepositoryAction::Commit(_))
            | (RepositoryAction::Stage(_), RepositoryAction::Stage(_))
            | (RepositoryAction::Unstage(_), RepositoryAction::Unstage(_))
            | (RepositoryAction::StageAll, RepositoryAction::StageAll)
            | (RepositoryAction::UnstageAll, RepositoryAction::UnstageAll)
    )
}
