use diffo_core::{SyncPlan, SyncProgress};

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
        OperationResult::Sync { plan } => sync_result_title(plan),
        OperationResult::Commit { hash } => format!("Committed {}", short_hash(hash)),
        OperationResult::Checkout { branch } => format!("Checked out {branch}"),
        OperationResult::CreateBranch { branch } => {
            format!("Created and checked out {branch}")
        }
    };
    Some((ToastKind::Success, title))
}

pub(crate) fn operation_failure_title(failure: &OperationFailure) -> String {
    let action = match &failure.action {
        RepositoryAction::Stage(_) | RepositoryAction::StageAll => "Stage",
        RepositoryAction::Unstage(_) | RepositoryAction::UnstageAll => "Unstage",
        RepositoryAction::Fetch => "Fetch",
        RepositoryAction::Sync => "Sync",
        RepositoryAction::Commit(_) => "Commit",
        RepositoryAction::Checkout(_) => "Checkout",
        RepositoryAction::CreateBranch(_) => "Create branch",
    };
    match failure.kind {
        FailureKind::PushRejected | FailureKind::HookRejected => {
            format!("Push rejected: {}", failure.detail)
        }
        FailureKind::RebaseConflict => failure.detail.clone(),
        _ => format!("{action} failed: {}", failure.detail),
    }
}

pub(crate) fn sync_plan_title(plan: &SyncPlan) -> String {
    let upstream = commit_count_sentence(&plan.upstream, plan.upstream_only, "upstream-only");
    let local = commit_count_sentence(&plan.branch, plan.local_only, "local-only");
    format!("{upstream} {local} {}", sync_plan_step(plan))
}

pub(crate) fn sync_progress_label(progress: &SyncProgress) -> String {
    match progress {
        SyncProgress::Fetching => "Fetching".to_owned(),
        SyncProgress::Plan(plan) => sync_plan_step(plan),
        SyncProgress::FastForwarding { branch } => format!("Fast-forwarding {branch}"),
        SyncProgress::Rebasing { commits } => format!("Rebasing {commits} commits"),
        SyncProgress::Pushing => "Pushing".to_owned(),
    }
}

fn sync_plan_step(plan: &SyncPlan) -> String {
    match (plan.local_only, plan.upstream_only) {
        (0, 0) => "Plan: finish after fetch; the branches have the same tip.".to_owned(),
        (0, _) => format!("Plan: fast-forward {} to {}.", plan.branch, plan.upstream),
        (_, 0) => format!("Plan: push {}.", plan.branch),
        (local, _) => format!(
            "Plan: rebase {local} {} onto {}, then push.",
            if local == 1 { "commit" } else { "commits" },
            plan.upstream
        ),
    }
}

fn commit_count_sentence(name: &str, count: usize, kind: &str) -> String {
    match count {
        0 => format!("{name} has no {kind} commits."),
        1 => format!("{name} has 1 {kind} commit."),
        _ => format!("{name} has {count} {kind} commits."),
    }
}

fn sync_result_title(plan: &SyncPlan) -> String {
    match (plan.local_only, plan.upstream_only) {
        (0, 0) => "Fetched; already up to date.".to_owned(),
        (0, commits) => format!(
            "Fast-forwarded {} by {commits} {}.",
            plan.branch,
            if commits == 1 { "commit" } else { "commits" }
        ),
        (_, 0) => format!("Pushed {}.", plan.branch),
        (commits, _) => format!(
            "Rebased {commits} {} and pushed {}.",
            if commits == 1 { "commit" } else { "commits" },
            plan.branch
        ),
    }
}

pub(super) fn short_hash(hash: &str) -> &str {
    hash.get(..7.min(hash.len())).unwrap_or(hash)
}
