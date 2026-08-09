use super::*;

#[test]
fn skips_local_hooks_without_changing_ordinary_git_commits() {
    let repo = test_repository();
    let hooks = repo.path().join(".git/hooks");
    let pre_commit_marker = repo.path().join(".git/pre-commit-ran");
    let commit_msg_marker = repo.path().join(".git/commit-msg-ran");
    for (name, marker) in [
        ("pre-commit", &pre_commit_marker),
        ("commit-msg", &commit_msg_marker),
    ] {
        let hook = hooks.join(name);
        fs::write(
            &hook,
            format!("#!/bin/sh\nprintf ran > '{}'\nexit 1\n", marker.display()),
        )
        .expect("write rejecting hook");
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))
            .expect("make hook executable");
    }
    fs::write(repo.path().join("tracked.txt"), "committed by Diffo\n")
        .expect("modify tracked file");
    git(repo.path(), &["add", "tracked.txt"]);
    let source = crate::GitRepositorySource::new(repo.path());

    let result = source
        .apply(&RepositoryAction::Commit("Diffo commit".to_owned()))
        .expect("Diffo commit should skip local hooks");

    assert!(matches!(result, OperationResult::Commit { .. }));
    assert_eq!(
        git_stdout(repo.path(), &["log", "-1", "--format=%s"]),
        "Diffo commit"
    );
    assert_eq!(
        git_stdout(repo.path(), &["show", "HEAD:tracked.txt"]),
        "committed by Diffo"
    );
    assert!(!pre_commit_marker.exists());
    assert!(!commit_msg_marker.exists());

    fs::write(repo.path().join("tracked.txt"), "ordinary Git commit\n")
        .expect("modify tracked file again");
    git(repo.path(), &["add", "tracked.txt"]);
    let output = Command::new("git")
        .args(["commit", "-m", "ordinary commit"])
        .current_dir(repo.path())
        .output()
        .expect("run ordinary git commit");

    assert!(!output.status.success());
    assert!(pre_commit_marker.exists());
    assert!(!commit_msg_marker.exists());
    assert_eq!(
        git_stdout(repo.path(), &["log", "-1", "--format=%s"]),
        "Diffo commit"
    );
}

#[test]
fn guarded_commit_accepts_the_captured_head_and_index() {
    let repo = test_repository();
    fs::write(repo.path().join("tracked.txt"), "guarded\n").expect("modify tracked file");
    git(repo.path(), &["add", "tracked.txt"]);
    let source = crate::GitRepositorySource::new(repo.path());
    let snapshot = source.snapshot().expect("captured snapshot");
    let expected_staged = snapshot.staged_files();
    let action = RepositoryAction::GuardedCommit(Box::new(GuardedCommitTarget {
        message: "feat: guarded commit".to_owned(),
        expected_head: snapshot.head,
        expected_staged,
    }));

    assert!(matches!(
        source.apply(&action),
        Ok(OperationResult::Commit { .. })
    ));
    assert_eq!(
        git_stdout(repo.path(), &["log", "-1", "--format=%s"]),
        "feat: guarded commit"
    );
}

#[test]
fn guarded_commit_rejects_an_index_changed_after_generation() {
    let repo = test_repository();
    fs::write(repo.path().join("tracked.txt"), "captured\n").expect("modify tracked file");
    git(repo.path(), &["add", "tracked.txt"]);
    let source = crate::GitRepositorySource::new(repo.path());
    let snapshot = source.snapshot().expect("captured snapshot");
    let expected_staged = snapshot.staged_files();
    let action = RepositoryAction::GuardedCommit(Box::new(GuardedCommitTarget {
        message: "must not commit".to_owned(),
        expected_head: snapshot.head,
        expected_staged,
    }));
    fs::write(repo.path().join("tracked.txt"), "changed again\n")
        .expect("change tracked file again");
    git(repo.path(), &["add", "tracked.txt"]);

    let failure = source.apply(&action).expect_err("stale index must fail");

    assert_eq!(failure.kind, FailureKind::RefChanged);
    assert!(failure.detail.contains("press i"));
    assert_eq!(
        git_stdout(repo.path(), &["log", "-1", "--format=%s"]),
        "Base commit"
    );
}

#[test]
fn guarded_commit_rejects_a_head_changed_after_generation() {
    let repo = test_repository();
    fs::write(repo.path().join("tracked.txt"), "captured\n").expect("modify tracked file");
    git(repo.path(), &["add", "tracked.txt"]);
    let source = crate::GitRepositorySource::new(repo.path());
    let snapshot = source.snapshot().expect("captured snapshot");
    let expected_staged = snapshot.staged_files();
    let action = RepositoryAction::GuardedCommit(Box::new(GuardedCommitTarget {
        message: "must not commit".to_owned(),
        expected_head: snapshot.head,
        expected_staged,
    }));
    fs::write(repo.path().join("head.txt"), "move head\n").expect("write head file");
    git(repo.path(), &["add", "head.txt"]);
    git(
        repo.path(),
        &["commit", "-m", "move head", "--", "head.txt"],
    );

    let failure = source.apply(&action).expect_err("stale HEAD must fail");

    assert_eq!(failure.kind, FailureKind::RefChanged);
    assert_eq!(
        git_stdout(repo.path(), &["log", "-1", "--format=%s"]),
        "move head"
    );
}
