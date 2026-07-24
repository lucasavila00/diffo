use diffo_core::{
    FailureKind, GitPrompt, OperationFailure, PromptAnswer, RepositoryAction, SyncPlan,
};

use crate::{GitRepositorySource, failure::operation_failure};

use super::{SyncExecution, sync_target::SyncTarget};

impl GitRepositorySource {
    pub(super) fn confirm_protected_push(
        &self,
        execution: &SyncExecution<'_>,
        plan: &SyncPlan,
        target: &SyncTarget,
        upstream_exists: bool,
    ) -> Result<bool, OperationFailure> {
        let action = execution.action;
        let Some(destination) =
            protected_push_destination(&target.remote, &target.upstream_branch, plan.local_only)
        else {
            return Ok(false);
        };
        let (Some(context), Some(bridge)) = (execution.context, execution.bridge) else {
            return Err(operation_failure(
                action,
                FailureKind::Unknown,
                "protected branch push confirmation is unavailable",
            ));
        };
        let local_tip = self
            .git_text(&["rev-parse", "HEAD"])
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        let upstream_tip = upstream_exists
            .then(|| self.git_text(&["rev-parse", &plan.upstream]))
            .transpose()
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        let worktree_status = self
            .git_text(&["status", "--porcelain=v1", "-z"])
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        let answer = context.prompts.prompt(
            bridge.next_prompt_id(),
            GitPrompt::ConfirmProtectedBranchPush {
                destination,
                commits: plan.local_only,
            },
            execution.cancellation,
        );
        if !matches!(answer, PromptAnswer::Confirm) || execution.cancellation.is_cancelled() {
            return Ok(true);
        }
        if !self.sync_plan_is_current(
            action,
            plan,
            target,
            &local_tip,
            upstream_tip.as_deref(),
            &worktree_status,
        ) {
            return Err(operation_failure(
                action,
                FailureKind::RefChanged,
                "repository state changed while confirming the push; start Sync again",
            ));
        }
        Ok(false)
    }

    #[allow(clippy::too_many_arguments)]
    fn sync_plan_is_current(
        &self,
        action: &RepositoryAction,
        plan: &SyncPlan,
        target: &SyncTarget,
        local_tip: &str,
        upstream_tip: Option<&str>,
        worktree_status: &str,
    ) -> bool {
        self.check_sync_starting_state(action).is_ok()
            && self
                .sync_branch(action)
                .is_ok_and(|current| current == plan.branch)
            && match (&target.original_upstream, &target.original_upstream_branch) {
                (Some(upstream), Some(upstream_branch)) => {
                    self.sync_upstream(action)
                        .is_ok_and(|current| current == *upstream)
                        && self
                            .git_text(&[
                                "config",
                                "--get",
                                &format!("branch.{}.remote", plan.branch),
                            ])
                            .is_ok_and(|current| current == target.remote)
                        && self
                            .git_text(&[
                                "config",
                                "--get",
                                &format!("branch.{}.merge", plan.branch),
                            ])
                            .is_ok_and(|current| current == *upstream_branch)
                }
                (None, None) => {
                    self.sync_upstream(action).is_err()
                        && self.remote_names().is_ok_and(|remotes| {
                            remotes.iter().any(|candidate| candidate == &target.remote)
                        })
                }
                _ => false,
            }
            && self
                .git_text(&["rev-parse", "HEAD"])
                .is_ok_and(|current| current == local_tip)
            && match upstream_tip {
                Some(expected) => self
                    .git_text(&["rev-parse", &plan.upstream])
                    .is_ok_and(|current| current == expected),
                None => self
                    .git(&["rev-parse", "--verify", "--quiet", &plan.upstream])
                    .is_err(),
            }
            && self
                .git_text(&["status", "--porcelain=v1", "-z"])
                .is_ok_and(|current| current == worktree_status)
    }
}

pub(crate) fn protected_push_destination(
    remote: &str,
    upstream_branch: &str,
    local_only: usize,
) -> Option<String> {
    if local_only == 0 {
        return None;
    }
    let branch = upstream_branch.strip_prefix("refs/heads/")?;
    matches!(branch, "main" | "master").then(|| format!("{remote}/{branch}"))
}
