use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use diffo_core::{
    CancellationHandle, FailureKind, HeadState, MergeRef, MergeRefKind, MergeTarget,
    OperationFailure, OperationOutcome, OperationResult, RepositoryAction,
    RepositoryOperationState,
};

use super::{
    GitRepositorySource,
    failure::{classify_failure, command_output, operation_failure},
    operation::{CommandOutcome, run_cancellable},
    status::parse_status,
};

const LOCAL_PREFIX: &str = "refs/heads/";
const REMOTE_PREFIX: &str = "refs/remotes/";
const TAG_PREFIX: &str = "refs/tags/";

impl GitRepositorySource {
    pub(super) fn merge_refs_list(&self) -> Result<Vec<MergeRef>> {
        let output = self.git(&[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(*objectname)%00%(symref)%00%(creatordate:unix)%00",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ])?;
        parse_merge_refs(&output)
    }

    pub(super) fn repository_operation_state(&self) -> RepositoryOperationState {
        if self.git_path_exists("MERGE_HEAD") {
            RepositoryOperationState::Merge
        } else if ["CHERRY_PICK_HEAD", "REVERT_HEAD"]
            .iter()
            .any(|name| self.git_path_exists(name))
            || ["rebase-merge", "rebase-apply"]
                .iter()
                .any(|name| self.git_path_exists(name))
        {
            RepositoryOperationState::Other
        } else {
            RepositoryOperationState::None
        }
    }

    fn git_path_exists(&self, name: &str) -> bool {
        let Ok(path) = self.git(&["rev-parse", "--git-path", name]) else {
            return false;
        };
        let path = String::from_utf8_lossy(&path);
        let path = std::path::Path::new(path.trim());
        if path.is_absolute() {
            path.exists()
        } else {
            self.root.join(path).exists()
        }
    }

    pub(super) fn apply_merge(
        &self,
        action: &RepositoryAction,
        cancellation: &CancellationHandle,
    ) -> Option<std::result::Result<OperationOutcome, OperationFailure>> {
        match action {
            RepositoryAction::Merge(target) => Some(self.merge(action, target, cancellation)),
            RepositoryAction::AbortMerge => Some(self.abort_merge(action, cancellation)),
            _ => None,
        }
    }

    fn merge(
        &self,
        action: &RepositoryAction,
        target: &MergeTarget,
        cancellation: &CancellationHandle,
    ) -> std::result::Result<OperationOutcome, OperationFailure> {
        self.verify_merge_target(action, target)?;
        let mut command = Command::new("git");
        command
            .args(["merge", "--no-edit", &target.full_ref])
            .current_dir(&self.root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .stdin(Stdio::null());
        match run_cancellable(&mut command, cancellation)
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?
        {
            CommandOutcome::Cancelled => Ok(OperationOutcome::Cancelled),
            CommandOutcome::Output(output) if output.status.success() => {
                Ok(OperationOutcome::Completed(OperationResult::Merge {
                    name: target.name.clone(),
                    conflicts: 0,
                }))
            }
            CommandOutcome::Output(output) => {
                let conflicts = self.conflicted_file_count();
                if conflicts > 0
                    && self.repository_operation_state() == RepositoryOperationState::Merge
                {
                    Ok(OperationOutcome::Completed(OperationResult::Merge {
                        name: target.name.clone(),
                        conflicts,
                    }))
                } else {
                    Err(classify_failure(action, &command_output(&output)))
                }
            }
        }
    }

    fn abort_merge(
        &self,
        action: &RepositoryAction,
        cancellation: &CancellationHandle,
    ) -> std::result::Result<OperationOutcome, OperationFailure> {
        if self.repository_operation_state() != RepositoryOperationState::Merge {
            return Err(operation_failure(
                action,
                FailureKind::OperationInProgress,
                "no merge is in progress",
            ));
        }
        let mut command = Command::new("git");
        command
            .args(["merge", "--abort"])
            .current_dir(&self.root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .stdin(Stdio::null());
        match run_cancellable(&mut command, cancellation)
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?
        {
            CommandOutcome::Cancelled => Ok(OperationOutcome::Cancelled),
            CommandOutcome::Output(output) if output.status.success() => {
                Ok(OperationOutcome::Completed(OperationResult::AbortMerge))
            }
            CommandOutcome::Output(output) => {
                Err(classify_failure(action, &command_output(&output)))
            }
        }
    }

    fn verify_merge_target(
        &self,
        action: &RepositoryAction,
        target: &MergeTarget,
    ) -> std::result::Result<(), OperationFailure> {
        if self.repository_operation_state() != RepositoryOperationState::None {
            return Err(operation_failure(
                action,
                FailureKind::OperationInProgress,
                "finish or abort the current Git operation before merging",
            ));
        }
        let expected_prefix = match target.kind {
            MergeRefKind::Local => LOCAL_PREFIX,
            MergeRefKind::Remote => REMOTE_PREFIX,
            MergeRefKind::Tag => TAG_PREFIX,
        };
        if !target.full_ref.starts_with(expected_prefix) {
            return Err(operation_failure(
                action,
                FailureKind::RefChanged,
                "selected ref is no longer available; reopen the merge command",
            ));
        }
        let object_id = self
            .git(&["show-ref", "--verify", "--hash", &target.full_ref])
            .ok()
            .and_then(|output| String::from_utf8(output).ok())
            .map(|output| output.trim().to_owned());
        let commit_spec = format!("{}^{{commit}}", target.full_ref);
        let commit_id = self
            .git(&["rev-parse", "--verify", &commit_spec])
            .ok()
            .and_then(|output| String::from_utf8(output).ok())
            .map(|output| output.trim().to_owned());
        if object_id.as_deref() != Some(target.object_id.as_str())
            || commit_id.as_deref() != Some(target.commit_id.as_str())
        {
            return Err(operation_failure(
                action,
                FailureKind::RefChanged,
                "selected ref changed; reopen the merge command",
            ));
        }
        let actual_head = self
            .git(&[
                "status",
                "--porcelain=v2",
                "--branch",
                "--untracked-files=no",
                "-z",
            ])
            .and_then(|status| parse_status(&status).map(|status| status.head))
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        if actual_head != target.expected_head {
            return Err(operation_failure(
                action,
                FailureKind::RefChanged,
                "HEAD changed; reopen the merge command",
            ));
        }
        if matches!(actual_head, HeadState::Unborn { .. }) {
            return Err(operation_failure(
                action,
                FailureKind::UnsupportedHead,
                "merge requires an existing commit",
            ));
        }
        Ok(())
    }
}

fn parse_merge_refs(output: &[u8]) -> Result<Vec<MergeRef>> {
    let mut refs = Vec::new();
    for record in output
        .split(|byte| *byte == b'\n')
        .filter(|row| !row.is_empty())
    {
        let fields = record.split(|byte| *byte == 0).collect::<Vec<_>>();
        let [full_ref, object_id, peeled_id, symref, commit_time, empty] = fields.as_slice() else {
            bail!("invalid git merge ref record")
        };
        if !empty.is_empty() {
            bail!("invalid git merge ref record terminator")
        }
        let full_ref = std::str::from_utf8(full_ref).context("git merge ref was not UTF-8")?;
        let object_id =
            std::str::from_utf8(object_id).context("git merge ref object ID was not UTF-8")?;
        let peeled_id =
            std::str::from_utf8(peeled_id).context("git merge ref commit ID was not UTF-8")?;
        let symref = std::str::from_utf8(symref).context("git merge ref symref was not UTF-8")?;
        let commit_time =
            std::str::from_utf8(commit_time).context("git merge ref time was not UTF-8")?;
        let (kind, name) = if let Some(name) = full_ref.strip_prefix(LOCAL_PREFIX) {
            (MergeRefKind::Local, name)
        } else if let Some(name) = full_ref.strip_prefix(REMOTE_PREFIX) {
            if !symref.is_empty() {
                continue;
            }
            (MergeRefKind::Remote, name)
        } else if let Some(name) = full_ref.strip_prefix(TAG_PREFIX) {
            (MergeRefKind::Tag, name)
        } else {
            bail!("git returned an unexpected merge ref: {full_ref}")
        };
        if name.is_empty() || object_id.is_empty() {
            bail!("git returned an incomplete merge ref")
        }
        refs.push(MergeRef {
            kind,
            name: name.to_owned(),
            full_ref: full_ref.to_owned(),
            object_id: object_id.to_owned(),
            commit_id: if peeled_id.is_empty() {
                object_id.to_owned()
            } else {
                peeled_id.to_owned()
            },
            tip_commit_unix_seconds: if commit_time.is_empty() {
                None
            } else {
                Some(
                    commit_time
                        .parse()
                        .context("git merge ref time was invalid")?,
                )
            },
        });
    }
    refs.sort_by(|left, right| {
        merge_ref_rank(left.kind)
            .cmp(&merge_ref_rank(right.kind))
            .then_with(|| {
                right
                    .tip_commit_unix_seconds
                    .cmp(&left.tip_commit_unix_seconds)
            })
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(refs)
}

const fn merge_ref_rank(kind: MergeRefKind) -> u8 {
    match kind {
        MergeRefKind::Local => 0,
        MergeRefKind::Remote => 1,
        MergeRefKind::Tag => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_groups_and_hides_remote_head() {
        let output = b"refs/tags/v1\x00tag\x00commit\x00\x00300\x00\nrefs/remotes/origin/topic\x00remote\x00\x00\x00400\x00\nrefs/remotes/origin/HEAD\x00main\x00\x00refs/remotes/origin/main\x00500\x00\nrefs/heads/topic\x00local\x00\x00\x00200\x00\n";

        let refs = parse_merge_refs(output).unwrap();

        assert_eq!(
            refs.iter()
                .map(|item| (item.kind, item.name.as_str(), item.commit_id.as_str()))
                .collect::<Vec<_>>(),
            [
                (MergeRefKind::Local, "topic", "local"),
                (MergeRefKind::Remote, "origin/topic", "remote"),
                (MergeRefKind::Tag, "v1", "commit"),
            ]
        );
    }
}
