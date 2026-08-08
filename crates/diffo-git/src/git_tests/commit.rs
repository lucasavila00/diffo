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
