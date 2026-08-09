use std::process::Command;

use diffo_core::{FailureKind, OperationFailure, RepositoryAction, RepositorySource};

use super::{
    GitRepositorySource, configure_checkout, configure_create_branch, configure_delete_branch,
    operation_failure,
};

pub(super) fn configure(
    source: &GitRepositorySource,
    command: &mut Command,
    action: &RepositoryAction,
) -> Result<(), OperationFailure> {
    if let RepositoryAction::GuardedCommit(target) = action {
        let snapshot = source
            .snapshot()
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        if snapshot.head != target.expected_head
            || snapshot.staged_files() != target.expected_staged
        {
            return Err(operation_failure(
                action,
                FailureKind::RefChanged,
                "staged changes changed; press i to generate a new commit message",
            ));
        }
    }

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
        RepositoryAction::Sync | RepositoryAction::SyncToRemote(_) => {
            unreachable!("sync is handled before single commands")
        }
        RepositoryAction::Commit(message) => {
            command.args(["commit", "--no-verify", "-m", message]);
        }
        RepositoryAction::GuardedCommit(target) => {
            command.args(["commit", "--no-verify", "-m", &target.message]);
        }
        RepositoryAction::Checkout(target) => {
            configure_checkout(source, command, action, target)?;
        }
        RepositoryAction::CreateBranch(target) => {
            configure_create_branch(source, command, action, target)?;
        }
        RepositoryAction::DeleteBranch(target) => {
            configure_delete_branch(source, command, action, target)?;
        }
        RepositoryAction::Merge(_) | RepositoryAction::AbortMerge => {
            unreachable!("merge actions are handled before single commands")
        }
        RepositoryAction::Discard(_)
        | RepositoryAction::DiscardAll(_)
        | RepositoryAction::Stash { .. }
        | RepositoryAction::ApplyStash(_)
        | RepositoryAction::DropStash(_)
        | RepositoryAction::Amend(_)
        | RepositoryAction::UndoLastCommit(_)
        | RepositoryAction::Revert(_)
        | RepositoryAction::RenameBranch(_) => {
            unreachable!("everyday actions are handled before single commands")
        }
    }
    Ok(())
}
