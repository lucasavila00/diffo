use super::*;

#[test]
fn first_sync_keeps_protected_branch_confirmation() {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "--bare", "remote.git"]);
    let repository = test_repository();
    git(
        repository.path(),
        &[
            "remote",
            "add",
            "origin",
            root.path().join("remote.git").to_str().unwrap(),
        ],
    );

    let failure = super::super::GitRepositorySource::new(repository.path())
        .apply(&RepositoryAction::Sync)
        .expect_err("protected first push needs confirmation");
    assert_eq!(
        failure.detail,
        "protected branch push confirmation is unavailable"
    );
    assert!(
        !Command::new("git")
            .args(["rev-parse", "--verify", "@{upstream}"])
            .current_dir(repository.path())
            .output()
            .unwrap()
            .status
            .success()
    );

    confirmed_sync(repository.path()).expect("confirm first protected push");
    assert_eq!(
        git_stdout(
            repository.path(),
            &["rev-parse", "--abbrev-ref", "@{upstream}"]
        ),
        "origin/main"
    );
}
