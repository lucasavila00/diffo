use std::process::{Command, Stdio};

use diffo_core::{FailureKind, OperationFailure, RepositoryAction, SyncPlan};

use crate::{
    GitRepositorySource,
    failure::{operation_failure, output_failure},
};

pub(super) struct SyncTarget {
    pub(super) remote: String,
    pub(super) upstream: String,
    pub(super) upstream_branch: String,
    pub(super) establish_upstream: bool,
    pub(super) original_upstream: Option<String>,
    pub(super) original_upstream_branch: Option<String>,
}

impl GitRepositorySource {
    pub(super) fn sync_upstream(
        &self,
        action: &RepositoryAction,
    ) -> Result<String, OperationFailure> {
        self.git_text(&[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ])
        .map_err(|_| {
            operation_failure(
                action,
                FailureKind::NoUpstream,
                "sync requires a configured upstream",
            )
        })
    }

    pub(super) fn sync_target(
        &self,
        action: &RepositoryAction,
        branch: &str,
        selected_remote: Option<&str>,
    ) -> Result<SyncTarget, OperationFailure> {
        if let Ok(upstream) = self.sync_upstream(action) {
            if selected_remote.is_some() {
                return Err(operation_failure(
                    action,
                    FailureKind::RefChanged,
                    "branch already has an upstream; start Sync again",
                ));
            }
            let remote = self
                .git_text(&["config", "--get", &format!("branch.{branch}.remote")])
                .map_err(|_| {
                    operation_failure(
                        action,
                        FailureKind::NoUpstream,
                        "sync requires a valid configured upstream",
                    )
                })?;
            let upstream_branch = self
                .git_text(&["config", "--get", &format!("branch.{branch}.merge")])
                .map_err(|_| {
                    operation_failure(
                        action,
                        FailureKind::NoUpstream,
                        "sync requires a valid configured upstream",
                    )
                })?;
            if !upstream_branch.starts_with("refs/heads/") {
                return Err(operation_failure(
                    action,
                    FailureKind::NoUpstream,
                    "sync requires an upstream branch",
                ));
            }
            let same_named_branch = format!("refs/heads/{branch}");
            let repair_upstream = upstream_branch != same_named_branch;
            return Ok(SyncTarget {
                upstream: if repair_upstream {
                    format!("{remote}/{branch}")
                } else {
                    upstream.clone()
                },
                upstream_branch: same_named_branch,
                establish_upstream: repair_upstream,
                original_upstream: Some(upstream),
                original_upstream_branch: Some(upstream_branch),
                remote,
            });
        }

        let remotes = self
            .remote_names()
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        let remote = if let Some(selected) = selected_remote {
            remotes
                .iter()
                .find(|remote| remote.as_str() == selected)
                .cloned()
                .ok_or_else(|| {
                    operation_failure(
                        action,
                        FailureKind::NoRemote,
                        "selected remote no longer exists; start Sync again",
                    )
                })?
        } else if remotes.iter().any(|remote| remote == "origin") {
            "origin".to_owned()
        } else if let [remote] = remotes.as_slice() {
            remote.clone()
        } else if remotes.is_empty() {
            return Err(operation_failure(
                action,
                FailureKind::NoRemote,
                "no remotes are configured; Sync does not create remotes",
            ));
        } else {
            return Err(operation_failure(
                action,
                FailureKind::NoRemote,
                "several remotes are configured; select one and start Sync again",
            ));
        };
        Ok(SyncTarget {
            upstream: format!("{remote}/{branch}"),
            upstream_branch: format!("refs/heads/{branch}"),
            remote,
            establish_upstream: true,
            original_upstream: None,
            original_upstream_branch: None,
        })
    }

    pub(super) fn check_sync_starting_state(
        &self,
        action: &RepositoryAction,
    ) -> Result<(), OperationFailure> {
        if ["MERGE_HEAD", "CHERRY_PICK_HEAD"].iter().any(|reference| {
            self.git(&["rev-parse", "--verify", "--quiet", reference])
                .is_ok()
        }) || ["rebase-merge", "rebase-apply"]
            .iter()
            .any(|name| self.git_path(name).is_some_and(|path| path.exists()))
        {
            return Err(operation_failure(
                action,
                FailureKind::OperationInProgress,
                "finish or abort the merge, rebase, or cherry-pick before syncing",
            ));
        }
        Ok(())
    }

    pub(super) fn sync_counts_against(
        &self,
        action: &RepositoryAction,
        upstream: &str,
    ) -> Result<(usize, usize), OperationFailure> {
        let range = format!("HEAD...{upstream}");
        let counts = self
            .git_text(&["rev-list", "--left-right", "--count", &range])
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        let mut counts = counts.split_whitespace();
        let local_only = counts.next().and_then(|value| value.parse().ok());
        let upstream_only = counts.next().and_then(|value| value.parse().ok());
        match (local_only, upstream_only, counts.next()) {
            (Some(local_only), Some(upstream_only), None) => Ok((local_only, upstream_only)),
            _ => Err(operation_failure(
                action,
                FailureKind::Unknown,
                "git returned invalid sync commit counts",
            )),
        }
    }

    pub(super) fn commit_count(
        &self,
        action: &RepositoryAction,
        reference: &str,
    ) -> Result<usize, OperationFailure> {
        self.git_text(&["rev-list", "--count", reference])
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?
            .parse()
            .map_err(|_| {
                operation_failure(
                    action,
                    FailureKind::Unknown,
                    "git returned an invalid commit count",
                )
            })
    }

    pub(super) fn require_related_histories(
        &self,
        action: &RepositoryAction,
        upstream: &str,
    ) -> Result<(), OperationFailure> {
        let output = Command::new("git")
            .args(["merge-base", "HEAD", upstream])
            .current_dir(&self.root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .output()
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        if output.status.success() {
            return Ok(());
        }
        if output.status.code() == Some(1) {
            return Err(output_failure(
                action,
                FailureKind::BranchConflict,
                "the local and remote branches have unrelated histories",
                &output,
            ));
        }
        Err(output_failure(
            action,
            FailureKind::Unknown,
            "could not compare the local and remote branch histories",
            &output,
        ))
    }

    pub(super) fn check_rebase_preconditions(
        &self,
        action: &RepositoryAction,
        plan: &SyncPlan,
    ) -> Result<(), OperationFailure> {
        if self.local_only_has_merge_commits(action, &plan.upstream)? {
            return Err(operation_failure(
                action,
                FailureKind::MergeCommits,
                "sync cannot rebase local-only history containing merge commits",
            ));
        }
        let tracked_status = self
            .git_text(&["status", "--porcelain", "--untracked-files=no"])
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        if !tracked_status.is_empty() {
            return Err(operation_failure(
                action,
                FailureKind::DirtyWorktree,
                "fetch completed, but staged or tracked changes must be committed, stashed, or discarded before rebase; nothing was pushed",
            ));
        }
        Ok(())
    }

    pub(super) fn set_sync_upstream(
        &self,
        action: &RepositoryAction,
        branch: &str,
        upstream: &str,
    ) -> Result<(), OperationFailure> {
        let assignment = format!("--set-upstream-to={upstream}");
        self.git(&["branch", &assignment, branch])
            .map(|_| ())
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))
    }
}
