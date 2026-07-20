use super::{
    FileKey, Model, OperationFailure, OperationResult, PendingFileAction, RepositoryAction,
    RepositorySnapshot, file_keys,
};

impl Model {
    pub fn show_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    pub fn start_repository_action(
        &mut self,
        action: RepositoryAction,
    ) -> Option<RepositoryAction> {
        if self.pending_operation.is_some() {
            return None;
        }
        self.error = None;
        self.pending_operation = Some(action.clone());
        Some(action)
    }

    pub fn repository_changed(&mut self, snapshot: RepositorySnapshot) {
        self.install_snapshot(snapshot, None);
    }

    pub(super) fn install_snapshot(
        &mut self,
        snapshot: RepositorySnapshot,
        intended_selection: Option<&FileKey>,
    ) {
        let old_selected = self.selected.clone();
        let old_cursor = self.cursor;
        let keys = file_keys(&snapshot);

        let cursor = intended_selection
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
    }

    pub(super) fn finish_pending_operation(&mut self) {
        if matches!(self.pending_operation, Some(RepositoryAction::Commit(_))) {
            self.commit_message.clear();
            self.commit_message_cursor = 0;
        }
        self.pending_operation = None;
    }

    pub fn complete_operation(
        &mut self,
        action: &RepositoryAction,
        result: &OperationResult,
        snapshot: RepositorySnapshot,
    ) -> bool {
        let is_async_result = matches!(
            result,
            OperationResult::Fetch { .. }
                | OperationResult::Sync { .. }
                | OperationResult::Commit { .. }
                | OperationResult::Checkout { .. }
                | OperationResult::CreateBranch { .. }
        );
        let finishes_pending = self.pending_operation.as_ref() == Some(action);
        if is_async_result && !finishes_pending {
            self.install_snapshot(snapshot, None);
            return false;
        }
        if finishes_pending {
            self.finish_pending_operation();
        }
        let intended_selection = self
            .pending_file_action
            .take_if(|pending| {
                pending.matches_repository_action(action) && pending.matches_result(result)
            })
            .and_then(|action| action.selection_after_success(&snapshot));
        self.install_snapshot(snapshot, intended_selection.as_ref());
        true
    }

    pub fn cancel_operation(&mut self, action: &RepositoryAction) {
        if self.pending_operation.as_ref() == Some(action) {
            self.pending_operation = None;
        }
        if self
            .pending_file_action
            .as_ref()
            .is_some_and(|pending| pending.matches_repository_action(action))
        {
            self.pending_file_action = None;
        }
        self.error = None;
    }

    pub fn fail_operation(&mut self, failure: &OperationFailure) -> bool {
        if let Some(pending) = self.pending_operation.as_ref()
            && !same_repository_operation(pending, &failure.action)
        {
            return false;
        }
        let pending_file_action_failed = self
            .pending_file_action
            .as_ref()
            .is_some_and(|pending| pending.matches_repository_action(&failure.action));
        self.pending_operation = None;
        if pending_file_action_failed {
            self.pending_file_action = None;
        }
        self.error = None;
        true
    }
}

impl PendingFileAction {
    pub(super) fn matches_result(&self, result: &OperationResult) -> bool {
        matches!(
            (self, result),
            (Self::StageFile(_), OperationResult::Stage)
                | (Self::UnstageFile(_), OperationResult::Unstage)
        )
    }

    pub(super) fn matches_repository_action(&self, action: &RepositoryAction) -> bool {
        match (self, action) {
            (Self::StageFile(pending), RepositoryAction::Stage(path)) => pending.path == *path,
            (Self::UnstageFile(pending), RepositoryAction::Unstage(path)) => pending.path == *path,
            _ => false,
        }
    }

    fn selection_after_success(self, snapshot: &RepositorySnapshot) -> Option<FileKey> {
        match self {
            Self::StageFile(action) => action
                .next_unstaged
                .filter(|target| file_keys(snapshot).contains(target))
                .or_else(|| {
                    file_keys(snapshot)
                        .into_iter()
                        .find(|key| key.area == super::ChangeArea::Unstaged)
                }),
            Self::UnstageFile(action) => action.next_staged,
        }
    }
}

pub(super) fn same_repository_operation(left: &RepositoryAction, right: &RepositoryAction) -> bool {
    matches!(
        (left, right),
        (RepositoryAction::Fetch, RepositoryAction::Fetch)
            | (RepositoryAction::Sync, RepositoryAction::Sync)
            | (RepositoryAction::Commit(_), RepositoryAction::Commit(_))
            | (RepositoryAction::Stage(_), RepositoryAction::Stage(_))
            | (RepositoryAction::Unstage(_), RepositoryAction::Unstage(_))
            | (RepositoryAction::StageAll, RepositoryAction::StageAll)
            | (RepositoryAction::UnstageAll, RepositoryAction::UnstageAll)
            | (RepositoryAction::Checkout(_), RepositoryAction::Checkout(_))
            | (
                RepositoryAction::CreateBranch(_),
                RepositoryAction::CreateBranch(_)
            )
    )
}
