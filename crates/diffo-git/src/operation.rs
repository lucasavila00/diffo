use std::{env, process::Command, thread, time::Duration};

use diffo_core::{
    FailureKind, OperationFailure, OperationResult, RepositoryAction, RepositorySource,
    UpstreamState,
};

use super::GitRepositorySource;

impl GitRepositorySource {
    pub(super) fn apply_operation(
        &self,
        action: &RepositoryAction,
    ) -> std::result::Result<OperationResult, OperationFailure> {
        if matches!(
            action,
            RepositoryAction::Fetch | RepositoryAction::Pull | RepositoryAction::Push
        ) && let Some(delay) = e2e_network_delay()
        {
            thread::sleep(delay);
        }

        let before_head = matches!(action, RepositoryAction::Pull)
            .then(|| self.git(&["rev-parse", "HEAD"]))
            .transpose()
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?
            .map(|head| String::from_utf8_lossy(&head).trim().to_owned());
        let before_fetch = matches!(action, RepositoryAction::Fetch)
            .then(|| self.snapshot().ok().and_then(|snapshot| snapshot.upstream))
            .flatten();
        if matches!(action, RepositoryAction::Push)
            && self
                .snapshot()
                .ok()
                .and_then(|snapshot| snapshot.upstream)
                .is_some_and(|upstream| upstream.behind > 0)
        {
            return Err(operation_failure(
                action,
                FailureKind::PullRequired,
                "pull required before push",
            ));
        }

        let mut command = Command::new("git");
        command
            .current_dir(&self.root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true");
        match action {
            RepositoryAction::Stage(path) => {
                command.args(["add", "--"]).arg(path);
            }
            RepositoryAction::Unstage(path) => {
                command.args(["reset", "--"]).arg(path);
            }
            RepositoryAction::StageAll => {
                command.args(["add", "--all"]);
            }
            RepositoryAction::UnstageAll => {
                command.arg("reset");
            }
            RepositoryAction::Fetch => {
                command.arg("fetch");
            }
            RepositoryAction::Pull => {
                command.args(["pull", "--ff-only"]);
            }
            RepositoryAction::Push => {
                command.args(["push", "--porcelain"]);
            }
            RepositoryAction::Commit(message) => {
                command.args(["commit", "-m", message]);
            }
        }

        let output = command
            .output()
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(classify_failure(action, &format!("{stdout}\n{stderr}")));
        }
        collect_operation_result(self, action, before_head, before_fetch.as_ref())
    }
}

fn collect_operation_result(
    source: &GitRepositorySource,
    action: &RepositoryAction,
    before_head: Option<String>,
    before_fetch: Option<&UpstreamState>,
) -> std::result::Result<OperationResult, OperationFailure> {
    match action {
        RepositoryAction::Stage(_) | RepositoryAction::StageAll => Ok(OperationResult::Stage),
        RepositoryAction::Unstage(_) | RepositoryAction::UnstageAll => Ok(OperationResult::Unstage),
        RepositoryAction::Fetch => {
            let after = source
                .snapshot()
                .ok()
                .and_then(|snapshot| snapshot.upstream);
            let updated_refs = usize::from(before_fetch != after.as_ref());
            Ok(OperationResult::Fetch { updated_refs })
        }
        RepositoryAction::Pull => {
            let old = before_head.unwrap_or_default();
            let new = source
                .git(&["rev-parse", "HEAD"])
                .map_err(|error| {
                    operation_failure(action, FailureKind::Unknown, &error.to_string())
                })
                .map(|head| String::from_utf8_lossy(&head).trim().to_owned())?;
            let range = format!("{old}..{new}");
            let commits = source
                .git(&["rev-list", "--count", &range])
                .ok()
                .and_then(|count| String::from_utf8(count).ok())
                .and_then(|count| count.trim().parse().ok())
                .unwrap_or(0);
            Ok(OperationResult::Pull { commits })
        }
        RepositoryAction::Push => {
            let snapshot = source.snapshot().map_err(|error| {
                operation_failure(action, FailureKind::Unknown, &error.to_string())
            })?;
            let hash = snapshot
                .recent_commits
                .first()
                .map_or_else(|| "unknown".to_owned(), |commit| commit.id.clone());
            let upstream = snapshot
                .upstream
                .map_or_else(|| "upstream".to_owned(), |upstream| upstream.name);
            Ok(OperationResult::Push { hash, upstream })
        }
        RepositoryAction::Commit(_) => {
            let hash = source
                .git(&["rev-parse", "HEAD"])
                .map_err(|error| {
                    operation_failure(action, FailureKind::Unknown, &error.to_string())
                })
                .map(|head| String::from_utf8_lossy(&head).trim().to_owned())?;
            Ok(OperationResult::Commit { hash })
        }
    }
}

fn operation_failure(
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
        (FailureKind::PushRejected, "remote changed; pull required")
    } else if text.contains("not possible to fast-forward") || text.contains("divergent branches") {
        (
            FailureKind::Diverged,
            "branches diverged; merge or rebase required",
        )
    } else if text.contains("hook declined") || text.contains("pre-receive hook") {
        (FailureKind::HookRejected, "rejected by remote hook")
    } else if text.contains("authentication")
        || text.contains("permission denied")
        || text.contains("could not read username")
    {
        (FailureKind::Authentication, "authentication required")
    } else if text.contains("conflict") {
        (FailureKind::MergeConflict, "resolve repository conflicts")
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
    } else {
        (FailureKind::Unknown, "Git operation failed")
    };
    operation_failure(action, kind, detail)
}

fn e2e_network_delay() -> Option<Duration> {
    let milliseconds = env::var("DIFFO_E2E_NETWORK_DELAY_MS")
        .ok()?
        .parse::<u64>()
        .ok()?
        .min(2_000);
    Some(Duration::from_millis(milliseconds))
}
