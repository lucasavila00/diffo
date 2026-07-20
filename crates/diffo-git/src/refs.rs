use std::{io, process::Command};

use diffo_core::{BranchKind, CheckoutTarget, FailureKind, OperationFailure, RepositoryAction};

use super::{GitRepositorySource, failure::operation_failure};

pub(super) fn ref_exists(source: &GitRepositorySource, full_ref: &str) -> io::Result<bool> {
    Command::new("git")
        .args(["show-ref", "--verify", "--quiet", full_ref])
        .current_dir(&source.root)
        .status()
        .map(|status| status.success())
}

pub(super) fn checkout_local_name(
    action: &RepositoryAction,
    target: &CheckoutTarget,
) -> std::result::Result<String, OperationFailure> {
    let name = match target.kind {
        BranchKind::Local => target.full_ref.strip_prefix("refs/heads/"),
        BranchKind::Remote => target
            .full_ref
            .strip_prefix("refs/remotes/")
            .and_then(|name| name.split_once('/').map(|(_, branch)| branch)),
    };
    name.filter(|name| !name.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            operation_failure(
                action,
                FailureKind::RefChanged,
                "selected branch ref is invalid",
            )
        })
}
