use std::process::Output;

use diffo_core::{FailureKind, OperationFailure, RepositoryAction};

use super::operation::CommandOutcome;

pub(super) fn operation_failure(
    action: &RepositoryAction,
    kind: FailureKind,
    detail: &str,
) -> OperationFailure {
    OperationFailure {
        action: action.clone(),
        kind,
        detail: detail.to_owned(),
    }
}

pub(super) fn classify_failure(action: &RepositoryAction, output: &str) -> OperationFailure {
    let text = output.to_ascii_lowercase();
    let (kind, detail) = if text.contains("non-fast-forward")
        || text.contains("fetch first")
        || text.contains("remote contains work")
    {
        (
            FailureKind::PushRejected,
            "remote changed; nothing was pushed",
        )
    } else if text.contains("hook declined") || text.contains("pre-receive hook") {
        (FailureKind::HookRejected, "rejected by remote hook")
    } else if text.contains("authentication")
        || text.contains("permission denied")
        || text.contains("could not read username")
    {
        (FailureKind::Authentication, "authentication required")
    } else if text.contains("conflict") {
        (
            FailureKind::RebaseConflict,
            "rebase conflicted and was aborted; nothing was pushed",
        )
    } else if text.contains("no configured push destination")
        || text.contains("does not appear to be a git repository")
        || text.contains("no such remote")
    {
        (FailureKind::NoRemote, "no remote configured")
    } else if text.contains("could not resolve host")
        || text.contains("connection refused")
        || text.contains("unable to access")
    {
        (FailureKind::Network, "network unavailable")
    } else if text.contains("local changes") || text.contains("would be overwritten") {
        (
            FailureKind::DirtyWorktree,
            "local changes block the operation",
        )
    } else if matches!(action, RepositoryAction::CreateBranch(_)) && text.contains("already exists")
    {
        (
            FailureKind::BranchConflict,
            "a local branch with that name already exists",
        )
    } else if matches!(action, RepositoryAction::DeleteBranch(target) if !target.force)
        && text.contains("not fully merged")
    {
        (
            FailureKind::BranchNotFullyMerged,
            "branch is not fully merged",
        )
    } else {
        (FailureKind::Unknown, "Git operation failed")
    };
    operation_failure(action, kind, detail)
}

pub(super) fn command_output(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

pub(super) fn finish_sync_command(
    action: &RepositoryAction,
    outcome: CommandOutcome,
) -> Result<bool, OperationFailure> {
    match outcome {
        CommandOutcome::Cancelled => Ok(true),
        CommandOutcome::Output(output) if !output.status.success() => {
            Err(classify_failure(action, &command_output(&output)))
        }
        CommandOutcome::Output(_) => Ok(false),
    }
}
