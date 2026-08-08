use std::process::{Command, Stdio};

use diffo_core::{
    AmendTarget, CancellationHandle, ChangeKind, Commit, DiscardAllTarget, DiscardTarget,
    FailureKind, OperationFailure, OperationOutcome, OperationResult, RenameBranchTarget,
    RepositoryAction, RepositorySource, StashEntry, UndoCommitTarget,
};

use super::{
    GitRepositorySource,
    failure::{classify_failure, command_output, operation_failure, output_failure},
    operation::{CommandOutcome, run_cancellable},
    refs::ref_exists,
};

impl GitRepositorySource {
    pub(super) fn apply_everyday(
        &self,
        action: &RepositoryAction,
        cancellation: &CancellationHandle,
    ) -> Option<Result<OperationOutcome, OperationFailure>> {
        let result = match action {
            RepositoryAction::Discard(target) => self.discard(action, target, cancellation),
            RepositoryAction::DiscardAll(target) => self.discard_all(action, target, cancellation),
            RepositoryAction::Stash { message } => self.stash(action, message, cancellation),
            RepositoryAction::ApplyStash(target) => self.apply_stash(action, target, cancellation),
            RepositoryAction::DropStash(target) => self.drop_stash(action, target, cancellation),
            RepositoryAction::Amend(target) => self.amend(action, target, cancellation),
            RepositoryAction::UndoLastCommit(target) => {
                self.undo_last_commit(action, target, cancellation)
            }
            RepositoryAction::Revert(target) => self.revert(action, target, cancellation),
            RepositoryAction::RenameBranch(target) => {
                self.rename_branch(action, target, cancellation)
            }
            _ => return None,
        };
        Some(result)
    }

    fn discard(
        &self,
        action: &RepositoryAction,
        target: &DiscardTarget,
        cancellation: &CancellationHandle,
    ) -> Result<OperationOutcome, OperationFailure> {
        let current = self
            .snapshot()
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?
            .files
            .into_iter()
            .find(|file| file.path == target.file.path);
        if current.as_ref() != Some(&target.file) {
            return Err(operation_failure(
                action,
                FailureKind::RefChanged,
                "selected file changed; review it before discarding",
            ));
        }
        let mut command = self.command();
        if target.file.kind == ChangeKind::Untracked {
            command.args(["clean", "-fd", "--"]).arg(&target.file.path);
        } else {
            command
                .args(["restore", "--worktree", "--"])
                .arg(&target.file.path);
        }
        Self::finish_command(
            action,
            &mut command,
            cancellation,
            OperationResult::Discard { paths: 1 },
        )
    }

    fn discard_all(
        &self,
        action: &RepositoryAction,
        target: &DiscardAllTarget,
        cancellation: &CancellationHandle,
    ) -> Result<OperationOutcome, OperationFailure> {
        let current = self
            .snapshot()
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?
            .files
            .into_iter()
            .filter(|file| file.unstaged.is_some() || file.kind == ChangeKind::Untracked)
            .collect::<Vec<_>>();
        if current != target.files {
            return Err(operation_failure(
                action,
                FailureKind::RefChanged,
                "working tree changed; review it before discarding",
            ));
        }
        if target.files.is_empty() {
            return Err(operation_failure(
                action,
                FailureKind::NothingToDo,
                "there are no unstaged or untracked changes",
            ));
        }
        let tracked = target
            .files
            .iter()
            .filter(|file| file.kind != ChangeKind::Untracked)
            .map(|file| &file.path)
            .collect::<Vec<_>>();
        let untracked = target
            .files
            .iter()
            .filter(|file| file.kind == ChangeKind::Untracked)
            .map(|file| &file.path)
            .collect::<Vec<_>>();
        if !tracked.is_empty() {
            let mut command = self.command();
            command.args(["restore", "--worktree", "--"]).args(tracked);
            match Self::run(action, &mut command, cancellation)? {
                CommandOutcome::Cancelled => return Ok(OperationOutcome::Cancelled),
                CommandOutcome::Output(output) if !output.status.success() => {
                    return Err(classify_failure(action, &output));
                }
                CommandOutcome::Output(_) => {}
            }
        }
        if !untracked.is_empty() {
            let mut command = self.command();
            command.args(["clean", "-fd", "--"]).args(untracked);
            match Self::run(action, &mut command, cancellation)? {
                CommandOutcome::Cancelled => return Ok(OperationOutcome::Cancelled),
                CommandOutcome::Output(output) if !output.status.success() => {
                    return Err(classify_failure(action, &output));
                }
                CommandOutcome::Output(_) => {}
            }
        }
        Ok(OperationOutcome::Completed(OperationResult::Discard {
            paths: target.files.len(),
        }))
    }

    fn stash(
        &self,
        action: &RepositoryAction,
        message: &str,
        cancellation: &CancellationHandle,
    ) -> Result<OperationOutcome, OperationFailure> {
        let before = self.git_text(&["rev-parse", "--verify", "refs/stash"]).ok();
        let mut command = self.command();
        command.args(["stash", "push", "--include-untracked"]);
        if !message.is_empty() {
            command.args(["--message", message]);
        }
        match Self::run(action, &mut command, cancellation)? {
            CommandOutcome::Cancelled => return Ok(OperationOutcome::Cancelled),
            CommandOutcome::Output(output) if !output.status.success() => {
                return Err(classify_failure(action, &output));
            }
            CommandOutcome::Output(_) => {}
        }
        let after = self.git_text(&["rev-parse", "--verify", "refs/stash"]).ok();
        if after.is_none() || after == before {
            return Err(operation_failure(
                action,
                FailureKind::NothingToDo,
                "there are no changes to stash",
            ));
        }
        Ok(OperationOutcome::Completed(OperationResult::Stash {
            name: "stash@{0}".to_owned(),
        }))
    }

    fn apply_stash(
        &self,
        action: &RepositoryAction,
        target: &StashEntry,
        cancellation: &CancellationHandle,
    ) -> Result<OperationOutcome, OperationFailure> {
        self.verify_stash(action, target)?;
        let mut command = self.command();
        command.args(["stash", "apply", "--index", &target.object_id]);
        match Self::run(action, &mut command, cancellation)? {
            CommandOutcome::Cancelled => Ok(OperationOutcome::Cancelled),
            CommandOutcome::Output(output) if output.status.success() => {
                Ok(OperationOutcome::Completed(OperationResult::ApplyStash {
                    name: target.name.clone(),
                }))
            }
            CommandOutcome::Output(output) => {
                let text = command_output(&output);
                if text.to_ascii_lowercase().contains("conflict") {
                    Err(output_failure(
                        action,
                        FailureKind::StashConflict,
                        "stash apply conflicted; the stash was kept",
                        &output,
                    ))
                } else {
                    Err(classify_failure(action, &output))
                }
            }
        }
    }

    fn drop_stash(
        &self,
        action: &RepositoryAction,
        target: &StashEntry,
        cancellation: &CancellationHandle,
    ) -> Result<OperationOutcome, OperationFailure> {
        self.verify_stash(action, target)?;
        let mut command = self.command();
        command.args(["stash", "drop", "--quiet", &target.name]);
        Self::finish_command(
            action,
            &mut command,
            cancellation,
            OperationResult::DropStash {
                name: target.name.clone(),
            },
        )
    }

    fn amend(
        &self,
        action: &RepositoryAction,
        target: &AmendTarget,
        cancellation: &CancellationHandle,
    ) -> Result<OperationOutcome, OperationFailure> {
        self.verify_rewritable_head(action, &target.expected_head)?;
        let mut command = self.command();
        command.args(["commit", "--amend", "-m", &target.message]);
        match Self::run(action, &mut command, cancellation)? {
            CommandOutcome::Cancelled => Ok(OperationOutcome::Cancelled),
            CommandOutcome::Output(output) if !output.status.success() => {
                Err(classify_failure(action, &output))
            }
            CommandOutcome::Output(_) => Ok(OperationOutcome::Completed(OperationResult::Amend {
                hash: self.head(action)?,
            })),
        }
    }

    fn undo_last_commit(
        &self,
        action: &RepositoryAction,
        target: &UndoCommitTarget,
        cancellation: &CancellationHandle,
    ) -> Result<OperationOutcome, OperationFailure> {
        self.verify_rewritable_head(action, &target.expected_head)?;
        let parent = self.git_text(&["rev-parse", "HEAD^"]).map_err(|_| {
            operation_failure(
                action,
                FailureKind::UnsupportedHead,
                "root commit cannot be undone",
            )
        })?;
        let mut command = self.command();
        command.args(["reset", "--soft", &parent]);
        Self::finish_command(
            action,
            &mut command,
            cancellation,
            OperationResult::UndoLastCommit { hash: parent },
        )
    }

    fn revert(
        &self,
        action: &RepositoryAction,
        target: &Commit,
        cancellation: &CancellationHandle,
    ) -> Result<OperationOutcome, OperationFailure> {
        if !self
            .git_text(&["status", "--porcelain"])
            .is_ok_and(|status| status.is_empty())
        {
            return Err(operation_failure(
                action,
                FailureKind::DirtyWorktree,
                "revert requires a clean worktree and index",
            ));
        }
        self.verify_non_merge_commit(action, &target.id)?;
        let reachable = Command::new("git")
            .args(["merge-base", "--is-ancestor", &target.id, "HEAD"])
            .current_dir(&self.root)
            .status()
            .is_ok_and(|status| status.success());
        if !reachable {
            return Err(operation_failure(
                action,
                FailureKind::RefChanged,
                "selected commit is no longer reachable from the current branch",
            ));
        }
        let before = self.head(action)?;
        let mut command = self.command();
        command.args(["revert", "--no-edit", &target.id]);
        match Self::run(action, &mut command, cancellation)? {
            CommandOutcome::Output(output) if output.status.success() => {
                Ok(OperationOutcome::Completed(OperationResult::Revert {
                    hash: self.head(action)?,
                }))
            }
            CommandOutcome::Cancelled => {
                self.abort_revert();
                Ok(OperationOutcome::Cancelled)
            }
            CommandOutcome::Output(output) => {
                self.abort_revert();
                if self.head(action).ok().as_deref() != Some(&before) {
                    return Err(output_failure(
                        action,
                        FailureKind::Unknown,
                        "revert failed and could not restore the original branch",
                        &output,
                    ));
                }
                Err(output_failure(
                    action,
                    FailureKind::RebaseConflict,
                    "revert conflicted and was aborted",
                    &output,
                ))
            }
        }
    }

    fn rename_branch(
        &self,
        action: &RepositoryAction,
        target: &RenameBranchTarget,
        cancellation: &CancellationHandle,
    ) -> Result<OperationOutcome, OperationFailure> {
        self.verify_current_branch(
            action,
            &target.old_name,
            &target.old_full_ref,
            &target.object_id,
        )?;
        self.verify_branch_name(action, &target.new_name)?;
        if ref_exists(self, &format!("refs/heads/{}", target.new_name))
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?
        {
            return Err(operation_failure(
                action,
                FailureKind::BranchConflict,
                "a local branch with that name already exists",
            ));
        }
        let has_upstream = self
            .git_text(&["rev-parse", "--verify", "@{upstream}"])
            .is_ok();
        if has_upstream != target.had_upstream {
            return Err(operation_failure(
                action,
                FailureKind::RefChanged,
                "branch upstream changed; reopen Rename Branch",
            ));
        }
        let mut rename = self.command();
        rename.args(["branch", "-m", &target.new_name]);
        match Self::run(action, &mut rename, cancellation)? {
            CommandOutcome::Cancelled => return Ok(OperationOutcome::Cancelled),
            CommandOutcome::Output(output) if !output.status.success() => {
                return Err(classify_failure(action, &output));
            }
            CommandOutcome::Output(_) => {}
        }
        if target.had_upstream {
            let mut unset = self.command();
            unset.args(["branch", "--unset-upstream", &target.new_name]);
            match Self::run(action, &mut unset, cancellation)? {
                CommandOutcome::Cancelled => return Ok(OperationOutcome::Cancelled),
                CommandOutcome::Output(output) if !output.status.success() => {
                    return Err(classify_failure(action, &output));
                }
                CommandOutcome::Output(_) => {}
            }
        }
        Ok(OperationOutcome::Completed(OperationResult::RenameBranch {
            branch: target.new_name.clone(),
        }))
    }

    fn verify_stash(
        &self,
        action: &RepositoryAction,
        target: &StashEntry,
    ) -> Result<(), OperationFailure> {
        let actual = self.git_text(&["rev-parse", "--verify", &target.name]).ok();
        if actual.as_deref() != Some(&target.object_id) {
            return Err(operation_failure(
                action,
                FailureKind::RefChanged,
                "selected stash changed; reopen the stash picker",
            ));
        }
        Ok(())
    }

    fn verify_rewritable_head(
        &self,
        action: &RepositoryAction,
        expected_head: &str,
    ) -> Result<(), OperationFailure> {
        if self.head(action)? != expected_head {
            return Err(operation_failure(
                action,
                FailureKind::RefChanged,
                "HEAD changed; reopen the command",
            ));
        }
        self.git_text(&["symbolic-ref", "--quiet", "HEAD"])
            .map_err(|_| {
                operation_failure(
                    action,
                    FailureKind::UnsupportedHead,
                    "operation requires a local branch",
                )
            })?;
        self.verify_non_merge_commit(action, expected_head)?;
        if self
            .git_text(&["rev-parse", "--verify", "@{upstream}"])
            .is_ok()
        {
            let local = self
                .git_text(&["rev-list", "--max-count=1", "@{upstream}..HEAD"])
                .map_err(|error| {
                    operation_failure(action, FailureKind::Unknown, &error.to_string())
                })?;
            if local != expected_head {
                return Err(operation_failure(
                    action,
                    FailureKind::PublishedCommit,
                    "the latest commit is not local-only",
                ));
            }
        }
        Ok(())
    }

    fn verify_non_merge_commit(
        &self,
        action: &RepositoryAction,
        commit: &str,
    ) -> Result<(), OperationFailure> {
        let parents = self
            .git_text(&["rev-list", "--parents", "--max-count=1", commit])
            .map_err(|error| {
                operation_failure(action, FailureKind::RefChanged, &error.to_string())
            })?;
        if parents.split_whitespace().count() > 2 {
            return Err(operation_failure(
                action,
                FailureKind::MergeCommits,
                "merge commits are not supported by this operation",
            ));
        }
        Ok(())
    }

    fn verify_current_branch(
        &self,
        action: &RepositoryAction,
        name: &str,
        full_ref: &str,
        object_id: &str,
    ) -> Result<(), OperationFailure> {
        if full_ref != format!("refs/heads/{name}") {
            return Err(operation_failure(
                action,
                FailureKind::RefChanged,
                "invalid branch ref",
            ));
        }
        let current = self
            .git_text(&["symbolic-ref", "--quiet", "--short", "HEAD"])
            .map_err(|_| {
                operation_failure(
                    action,
                    FailureKind::UnsupportedHead,
                    "operation requires a local branch",
                )
            })?;
        let actual = self.git_text(&["rev-parse", "--verify", full_ref]).ok();
        if current != name || actual.as_deref() != Some(object_id) {
            return Err(operation_failure(
                action,
                FailureKind::RefChanged,
                "current branch changed; reopen the command",
            ));
        }
        Ok(())
    }

    fn verify_branch_name(
        &self,
        action: &RepositoryAction,
        name: &str,
    ) -> Result<(), OperationFailure> {
        let valid = Command::new("git")
            .args(["check-ref-format", "--branch", name])
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if !valid {
            return Err(operation_failure(
                action,
                FailureKind::BranchConflict,
                "invalid branch name",
            ));
        }
        Ok(())
    }

    fn head(&self, action: &RepositoryAction) -> Result<String, OperationFailure> {
        self.git_text(&["rev-parse", "--verify", "HEAD"])
            .map_err(|error| {
                operation_failure(action, FailureKind::UnsupportedHead, &error.to_string())
            })
    }

    fn abort_revert(&self) {
        let _ = Command::new("git")
            .args(["revert", "--abort"])
            .current_dir(&self.root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .stdin(Stdio::null())
            .output();
    }

    fn command(&self) -> Command {
        let mut command = Command::new("git");
        command
            .current_dir(&self.root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .stdin(Stdio::null());
        command
    }

    fn run(
        action: &RepositoryAction,
        command: &mut Command,
        cancellation: &CancellationHandle,
    ) -> Result<CommandOutcome, OperationFailure> {
        run_cancellable(command, cancellation)
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))
    }

    fn finish_command(
        action: &RepositoryAction,
        command: &mut Command,
        cancellation: &CancellationHandle,
        result: OperationResult,
    ) -> Result<OperationOutcome, OperationFailure> {
        match Self::run(action, command, cancellation)? {
            CommandOutcome::Cancelled => Ok(OperationOutcome::Cancelled),
            CommandOutcome::Output(output) if !output.status.success() => {
                Err(classify_failure(action, &output))
            }
            CommandOutcome::Output(_) => Ok(OperationOutcome::Completed(result)),
        }
    }
}
