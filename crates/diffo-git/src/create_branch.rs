use std::process::{Command, Stdio};

use diffo_core::{
    CreateBranchStartPoint, CreateBranchTarget, FailureKind, HeadState, OperationFailure,
    OperationResult, RepositoryAction,
};

use super::{
    GitRepositorySource, failure::operation_failure, operation::verify_checkout_target,
    refs::ref_exists, status::parse_status,
};

pub(super) fn configure_create_branch(
    source: &GitRepositorySource,
    command: &mut Command,
    action: &RepositoryAction,
    target: &CreateBranchTarget,
) -> std::result::Result<(), OperationFailure> {
    let valid_name = Command::new("git")
        .args(["check-ref-format", "--branch", &target.name])
        .current_dir(&source.root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?
        .success();
    if !valid_name {
        return Err(operation_failure(
            action,
            FailureKind::BranchConflict,
            "invalid branch name",
        ));
    }
    if ref_exists(source, &format!("refs/heads/{}", target.name))
        .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?
    {
        return Err(operation_failure(
            action,
            FailureKind::BranchConflict,
            "a local branch with that name already exists",
        ));
    }
    let commit = match &target.start_point {
        CreateBranchStartPoint::Head(expected_head) => {
            let actual_head = source
                .git(&[
                    "status",
                    "--porcelain=v2",
                    "--branch",
                    "--untracked-files=no",
                    "-z",
                ])
                .and_then(|status| parse_status(&status).map(|status| status.head))
                .map_err(|error| {
                    operation_failure(action, FailureKind::Unknown, &error.to_string())
                })?;
            if actual_head != *expected_head {
                return Err(operation_failure(
                    action,
                    FailureKind::RefChanged,
                    "HEAD changed; reopen the create branch command",
                ));
            }
            match expected_head {
                HeadState::Named { commit, .. } | HeadState::Detached { commit } => commit,
                HeadState::Unborn { .. } => {
                    return Err(operation_failure(
                        action,
                        FailureKind::UnsupportedHead,
                        "create branch requires a commit",
                    ));
                }
            }
        }
        CreateBranchStartPoint::Branch(branch) => {
            verify_checkout_target(source, action, branch)?;
            &branch.object_id
        }
    };
    command.args(["checkout", "-q", "-b", &target.name, "--no-track", commit]);
    Ok(())
}

pub(super) fn operation_result(target: &CreateBranchTarget) -> OperationResult {
    OperationResult::CreateBranch {
        branch: target.name.clone(),
    }
}
