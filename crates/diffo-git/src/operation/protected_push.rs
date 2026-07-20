use diffo_core::{
    CancellationHandle, FailureKind, GitPrompt, OperationFailure, PromptAnswer, RepositoryAction,
    RepositoryOperationContext, SyncPlan,
};

use crate::{GitRepositorySource, askpass::AskpassBridge, failure::operation_failure};

impl GitRepositorySource {
    pub(super) fn confirm_protected_push(
        &self,
        plan: &SyncPlan,
        remote: &str,
        upstream_branch: &str,
        context: Option<&RepositoryOperationContext>,
        cancellation: &CancellationHandle,
        bridge: Option<&AskpassBridge>,
    ) -> Result<bool, OperationFailure> {
        let Some(destination) =
            protected_push_destination(remote, upstream_branch, plan.local_only)
        else {
            return Ok(false);
        };
        let (Some(context), Some(bridge)) = (context, bridge) else {
            return Err(operation_failure(
                &RepositoryAction::Sync,
                FailureKind::Unknown,
                "protected branch push confirmation is unavailable",
            ));
        };
        let action = &RepositoryAction::Sync;
        let local_tip = self
            .git_text(&["rev-parse", "HEAD"])
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        let upstream_tip = self
            .git_text(&["rev-parse", &plan.upstream])
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        let answer = context.prompts.prompt(
            bridge.next_prompt_id(),
            GitPrompt::ConfirmProtectedBranchPush {
                destination,
                commits: plan.local_only,
            },
            cancellation,
        );
        if !matches!(answer, PromptAnswer::Confirm) || cancellation.is_cancelled() {
            return Ok(true);
        }
        if !self.sync_plan_is_current(
            action,
            &plan.branch,
            &plan.upstream,
            remote,
            upstream_branch,
            &local_tip,
            &upstream_tip,
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
        branch: &str,
        upstream: &str,
        remote: &str,
        upstream_branch: &str,
        local_tip: &str,
        upstream_tip: &str,
    ) -> bool {
        self.check_sync_starting_state(action).is_ok()
            && self
                .sync_branch(action)
                .is_ok_and(|current| current == branch)
            && self
                .sync_upstream(action)
                .is_ok_and(|current| current == upstream)
            && self
                .git_text(&["config", "--get", &format!("branch.{branch}.remote")])
                .is_ok_and(|current| current == remote)
            && self
                .git_text(&["config", "--get", &format!("branch.{branch}.merge")])
                .is_ok_and(|current| current == upstream_branch)
            && self
                .git_text(&["rev-parse", "HEAD"])
                .is_ok_and(|current| current == local_tip)
            && self
                .git_text(&["rev-parse", upstream])
                .is_ok_and(|current| current == upstream_tip)
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
