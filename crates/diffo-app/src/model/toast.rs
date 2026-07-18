use super::repository::same_repository_operation;
use super::{
    CommitComposerState, FailureKind, Model, OperationFailure, OperationResult, RepositoryAction,
    Toast, ToastKind,
};

impl Model {
    pub fn show_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    pub fn show_operation_failure(&mut self, failure: &OperationFailure) {
        if let Some(pending) = self.pending_operation.as_ref()
            && !same_repository_operation(pending, &failure.action)
        {
            return;
        }
        let pending_file_action_failed = self
            .pending_file_action
            .as_ref()
            .is_some_and(|pending| pending.matches_repository_action(&failure.action));
        if matches!(self.pending_operation, Some(RepositoryAction::Commit(_))) {
            self.commit_composer_state = CommitComposerState::Focused;
        }
        self.pending_operation = None;
        if pending_file_action_failed {
            self.pending_file_action = None;
        }
        self.error = None;
        self.push_toast(ToastKind::Error, operation_failure_title(failure), None);
    }

    pub fn dismiss_toast(&mut self, id: u64) {
        self.toasts.retain(|toast| toast.id != id);
    }

    pub fn show_toast(&mut self, kind: ToastKind, title: impl Into<String>) {
        self.push_toast(kind, title.into(), None);
    }

    pub(super) fn push_toast(&mut self, kind: ToastKind, title: String, detail: Option<String>) {
        self.toasts
            .retain(|toast| toast.title != title || toast.detail != detail);
        let toast = Toast {
            id: self.next_toast_id,
            kind,
            title,
            detail,
        };
        self.next_toast_id = self.next_toast_id.saturating_add(1);
        self.toasts.insert(0, toast);
        self.toasts.truncate(3);
    }
}

pub(super) fn operation_result_toast(result: &OperationResult) -> Option<(ToastKind, String)> {
    let title = match result {
        OperationResult::Stage | OperationResult::Unstage => return None,
        OperationResult::Fetch { updated_refs: 0 } => "Fetch complete".to_owned(),
        OperationResult::Fetch { updated_refs: 1 } => "Fetched 1 ref".to_owned(),
        OperationResult::Fetch { updated_refs } => format!("Fetched {updated_refs} refs"),
        OperationResult::Pull { commits: 0 } => "Already up to date".to_owned(),
        OperationResult::Pull { commits: 1 } => "Pulled 1 commit".to_owned(),
        OperationResult::Pull { commits } => format!("Pulled {commits} commits"),
        OperationResult::Push { hash, upstream } => {
            format!("Pushed {} to {upstream}", short_hash(hash))
        }
        OperationResult::Commit { hash } => format!("Committed {}", short_hash(hash)),
    };
    Some((ToastKind::Success, title))
}

pub(super) fn operation_failure_title(failure: &OperationFailure) -> String {
    let action = match &failure.action {
        RepositoryAction::Stage(_) | RepositoryAction::StageAll => "Stage",
        RepositoryAction::Unstage(_) | RepositoryAction::UnstageAll => "Unstage",
        RepositoryAction::Fetch => "Fetch",
        RepositoryAction::Pull => "Pull",
        RepositoryAction::Push => "Push",
        RepositoryAction::Commit(_) => "Commit",
    };
    match failure.kind {
        FailureKind::PullRequired => format!("Push blocked: {}", failure.detail),
        FailureKind::Diverged => format!("Pull blocked: {}", failure.detail),
        FailureKind::PushRejected | FailureKind::HookRejected => {
            format!("Push rejected: {}", failure.detail)
        }
        FailureKind::MergeConflict => format!("Pull stopped: {}", failure.detail),
        _ => format!("{action} failed: {}", failure.detail),
    }
}

pub(super) fn short_hash(hash: &str) -> &str {
    hash.get(..7.min(hash.len())).unwrap_or(hash)
}
