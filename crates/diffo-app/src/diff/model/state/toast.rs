use super::{FailureKind, OperationFailure, OperationResult, RepositoryAction, Toast, ToastKind};

pub struct ToastQueue {
    toasts: Vec<Toast>,
    next_id: u64,
}

impl ToastQueue {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            toasts: Vec::new(),
            next_id: 1,
        }
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Toast] {
        &self.toasts
    }

    pub fn dismiss(&mut self, id: u64) {
        self.toasts.retain(|toast| toast.id != id);
    }

    pub fn show(&mut self, kind: ToastKind, title: impl Into<String>) -> u64 {
        let title = title.into();
        let detail = None;
        self.toasts
            .retain(|toast| toast.title != title || toast.detail != detail);
        let toast = Toast {
            id: self.next_id,
            kind,
            title,
            detail,
        };
        self.next_id = self.next_id.saturating_add(1);
        self.toasts.insert(0, toast);
        self.toasts.truncate(3);
        self.next_id.saturating_sub(1)
    }
}

impl Default for ToastQueue {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn operation_result_toast(result: &OperationResult) -> Option<(ToastKind, String)> {
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
        OperationResult::Checkout { branch } => format!("Checked out {branch}"),
    };
    Some((ToastKind::Success, title))
}

pub(crate) fn operation_failure_title(failure: &OperationFailure) -> String {
    let action = match &failure.action {
        RepositoryAction::Stage(_) | RepositoryAction::StageAll => "Stage",
        RepositoryAction::Unstage(_) | RepositoryAction::UnstageAll => "Unstage",
        RepositoryAction::Fetch => "Fetch",
        RepositoryAction::Pull => "Pull",
        RepositoryAction::Push => "Push",
        RepositoryAction::Commit(_) => "Commit",
        RepositoryAction::Checkout(_) => "Checkout",
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
