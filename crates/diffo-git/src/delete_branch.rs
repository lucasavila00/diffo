use std::process::Command;

use diffo_core::{
    BranchKind, CheckoutTarget, DeleteBranchTarget, FailureKind, OperationFailure, OperationResult,
    RepositoryAction,
};

use super::{GitRepositorySource, failure::operation_failure, operation::verify_checkout_target};

pub(super) fn configure_delete_branch(
    source: &GitRepositorySource,
    command: &mut Command,
    action: &RepositoryAction,
    target: &DeleteBranchTarget,
) -> Result<(), OperationFailure> {
    let name = target
        .full_ref
        .strip_prefix("refs/heads/")
        .filter(|name| !name.is_empty() && *name == target.name)
        .ok_or_else(|| {
            operation_failure(
                action,
                FailureKind::RefChanged,
                "selected branch ref is invalid; reopen the branch picker",
            )
        })?;
    verify_checkout_target(
        source,
        action,
        &CheckoutTarget {
            kind: BranchKind::Local,
            full_ref: target.full_ref.clone(),
            object_id: target.object_id.clone(),
        },
    )?;

    let current = Command::new("git")
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .current_dir(&source.root)
        .env("LC_ALL", "C")
        .output()
        .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
    if current.status.success() {
        let current = String::from_utf8(current.stdout).map_err(|_| {
            operation_failure(
                action,
                FailureKind::Unknown,
                "git returned a non-UTF-8 current branch",
            )
        })?;
        if current.trim() == name {
            return Err(operation_failure(
                action,
                FailureKind::RefChanged,
                "the current branch cannot be deleted; reopen the branch picker",
            ));
        }
    } else if current.status.code() != Some(1) {
        return Err(operation_failure(
            action,
            FailureKind::Unknown,
            "could not determine the current branch",
        ));
    }

    command
        .env("LC_ALL", "C")
        .args(["branch", if target.force { "-D" } else { "-d" }, "--", name]);
    Ok(())
}

pub(super) fn operation_result(target: &DeleteBranchTarget) -> OperationResult {
    OperationResult::DeleteBranch {
        branch: target.name.clone(),
    }
}
