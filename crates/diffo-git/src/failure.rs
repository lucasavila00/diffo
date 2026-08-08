use std::process::Output;

use diffo_core::{FailureKind, OperationFailure, RepositoryAction};

use super::operation::CommandOutcome;

const MAX_GIT_FAILURE_DETAIL_BYTES: usize = 16 * 1024;
const TRUNCATION_NOTICE: &str = "\n\n[Git diagnostic truncated]";

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

pub(super) fn classify_failure(action: &RepositoryAction, output: &Output) -> OperationFailure {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    let (kind, summary) = if text.contains("non-fast-forward")
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
    output_failure(action, kind, summary, output)
}

pub(super) fn output_failure(
    action: &RepositoryAction,
    kind: FailureKind,
    summary: &str,
    output: &Output,
) -> OperationFailure {
    let expose_output = !matches!(
        kind,
        FailureKind::Authentication | FailureKind::HookRejected
    );
    let detail = git_failure_detail(summary, output, expose_output);
    operation_failure(action, kind, &detail)
}

pub(super) fn command_output(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn git_failure_detail(summary: &str, output: &Output, expose_output: bool) -> String {
    let mut detail = summary.to_owned();
    detail.push_str("\n\nGit ");
    detail.push_str(&output.status.to_string());
    detail.push('.');

    if expose_output {
        append_stream(
            &mut detail,
            "stderr",
            &String::from_utf8_lossy(&output.stderr),
            summary,
        );
        append_stream(
            &mut detail,
            "stdout",
            &String::from_utf8_lossy(&output.stdout),
            summary,
        );
    }

    truncate_detail(detail)
}

fn append_stream(detail: &mut String, label: &str, stream: &str, summary: &str) {
    let stream = stream.trim();
    if stream.is_empty() || stream == summary {
        return;
    }
    detail.push_str("\n\n");
    detail.push_str(label);
    detail.push_str(":\n");
    detail.push_str(stream);
}

fn truncate_detail(mut detail: String) -> String {
    if detail.len() <= MAX_GIT_FAILURE_DETAIL_BYTES {
        return detail;
    }
    let mut end = MAX_GIT_FAILURE_DETAIL_BYTES.saturating_sub(TRUNCATION_NOTICE.len());
    while !detail.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    detail.truncate(end);
    detail.push_str(TRUNCATION_NOTICE);
    detail
}

pub(super) fn finish_sync_command(
    action: &RepositoryAction,
    outcome: CommandOutcome,
) -> Result<bool, OperationFailure> {
    match outcome {
        CommandOutcome::Cancelled => Ok(true),
        CommandOutcome::Output(output) if !output.status.success() => {
            Err(classify_failure(action, &output))
        }
        CommandOutcome::Output(_) => Ok(false),
    }
}
