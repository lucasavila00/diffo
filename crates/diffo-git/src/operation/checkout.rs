use diffo_core::{BranchKind, CheckoutTarget, FailureKind, OperationFailure, RepositoryAction};

use crate::{GitRepositorySource, failure::operation_failure};

pub(crate) fn verify_checkout_target(
    source: &GitRepositorySource,
    action: &RepositoryAction,
    target: &CheckoutTarget,
) -> Result<(), OperationFailure> {
    let expected_prefix = match target.kind {
        BranchKind::Local => "refs/heads/",
        BranchKind::Remote => "refs/remotes/",
    };
    if !target.full_ref.starts_with(expected_prefix) {
        return Err(operation_failure(
            action,
            FailureKind::RefChanged,
            "selected branch is no longer available; reopen the branch picker",
        ));
    }
    let object_id = source
        .git(&["show-ref", "--verify", "--hash", &target.full_ref])
        .ok()
        .and_then(|output| String::from_utf8(output).ok())
        .map(|output| output.trim().to_owned());
    if object_id.as_deref() != Some(target.object_id.as_str()) {
        return Err(operation_failure(
            action,
            FailureKind::RefChanged,
            "selected branch changed; reopen the branch picker",
        ));
    }
    Ok(())
}
