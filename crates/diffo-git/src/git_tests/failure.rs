use std::os::unix::process::ExitStatusExt as _;

use super::*;
use crate::failure::classify_failure;

fn failed_output(stdout: &str, stderr: &str) -> std::process::Output {
    std::process::Output {
        status: std::process::ExitStatus::from_raw(7 << 8),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

#[test]
fn classifies_failures_and_redacts_sensitive_git_output() {
    for (action, stderr, expected, exposes_output) in [
        (
            RepositoryAction::Sync,
            "[rejected] (non-fast-forward)",
            FailureKind::PushRejected,
            true,
        ),
        (
            RepositoryAction::Sync,
            "pre-receive hook declined: token=secret",
            FailureKind::HookRejected,
            false,
        ),
        (
            RepositoryAction::Sync,
            "CONFLICT in file",
            FailureKind::RebaseConflict,
            true,
        ),
        (
            RepositoryAction::Fetch,
            "fatal: could not resolve host",
            FailureKind::Network,
            true,
        ),
        (
            RepositoryAction::Fetch,
            "authentication failed: token=secret",
            FailureKind::Authentication,
            false,
        ),
    ] {
        let failure = classify_failure(&action, &failed_output("", stderr));
        assert_eq!(failure.kind, expected);
        assert!(failure.detail.contains("Git exit status: 7"));
        assert_eq!(failure.detail.contains(stderr), exposes_output);
        assert!(!failure.detail.contains("secret"));
    }
}

#[test]
fn unknown_failure_keeps_status_stderr_and_stdout_separate() {
    let failure = classify_failure(
        &RepositoryAction::Fetch,
        &failed_output("  stdout detail\n", "\nstderr detail  "),
    );

    assert_eq!(failure.kind, FailureKind::Unknown);
    assert_eq!(
        failure.detail,
        "Git operation failed\n\nGit exit status: 7.\n\nstderr:\nstderr detail\n\nstdout:\nstdout detail"
    );
}

#[test]
fn diagnostic_is_bounded_and_marks_truncation() {
    let failure = classify_failure(
        &RepositoryAction::Fetch,
        &failed_output("stdout", &"é".repeat(16 * 1024)),
    );

    assert!(failure.detail.len() <= 16 * 1024);
    assert!(failure.detail.ends_with("[Git diagnostic truncated]"));
}

#[test]
fn real_git_failure_reaches_the_operation_with_its_diagnostic() {
    let repo = test_repository();
    let failure = crate::GitRepositorySource::new(repo.path())
        .apply(&RepositoryAction::Stage(PathBuf::from("missing.txt")))
        .expect_err("staging a missing path should fail");

    assert_eq!(failure.kind, FailureKind::Unknown);
    assert!(failure.detail.contains("Git exit status:"));
    assert!(failure.detail.contains("stderr:"));
    assert!(
        failure
            .detail
            .contains("pathspec 'missing.txt' did not match")
    );
}
