use super::*;

#[test]
fn sync_repairs_a_mismatched_upstream_without_pushing_its_branch() {
    let repository = sync_repository();
    let master_before = remote_head(&repository.seed);
    git(
        &repository.work,
        &["switch", "-c", "feature/nested", "origin/master"],
    );
    fs::write(repository.work.join("feature.txt"), "feature\n").unwrap();
    git(&repository.work, &["add", "."]);
    git(&repository.work, &["commit", "-m", "Feature commit"]);
    let feature = git_stdout(&repository.work, &["rev-parse", "HEAD"]);

    let result = super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect("repair mismatched upstream");

    assert!(matches!(
        result,
        OperationResult::Sync { plan }
            if plan.branch == "feature/nested"
                && plan.upstream == "origin/feature/nested"
                && plan.establish_upstream
    ));
    assert_eq!(remote_head(&repository.seed), master_before);
    assert_eq!(
        git_stdout(
            &repository.work,
            &["ls-remote", "--refs", "origin", "refs/heads/feature/nested"]
        )
        .split_whitespace()
        .next(),
        Some(feature.as_str())
    );
    assert_eq!(
        git_stdout(
            &repository.work,
            &["rev-parse", "--abbrev-ref", "@{upstream}"]
        ),
        "origin/feature/nested"
    );
}

#[test]
fn rejected_mismatched_upstream_push_keeps_the_original_upstream() {
    let repository = sync_repository();
    git(
        &repository.work,
        &["switch", "-c", "topic", "origin/master"],
    );
    fs::write(repository.work.join("topic.txt"), "topic\n").unwrap();
    git(&repository.work, &["add", "."]);
    git(&repository.work, &["commit", "-m", "Topic commit"]);
    let hook = repository.root.path().join("remote.git/hooks/pre-receive");
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();

    let failure = super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect_err("hook must reject repaired destination");

    assert_eq!(failure.kind, FailureKind::HookRejected);
    assert_eq!(
        git_stdout(
            &repository.work,
            &["rev-parse", "--abbrev-ref", "@{upstream}"]
        ),
        "origin/master"
    );
    assert!(
        Command::new("git")
            .args([
                "ls-remote",
                "--exit-code",
                "--refs",
                "origin",
                "refs/heads/topic"
            ])
            .current_dir(&repository.work)
            .output()
            .unwrap()
            .status
            .code()
            .is_some_and(|code| code == 2)
    );
}

#[test]
fn mismatched_protected_upstream_is_repaired_without_confirmation() {
    let repository = sync_repository();
    git(&repository.work, &["branch", "-m", "work"]);
    fs::write(repository.work.join("local.txt"), "local\n").unwrap();
    git(&repository.work, &["add", "."]);
    git(&repository.work, &["commit", "-m", "Local commit"]);
    let local_before = git_stdout(&repository.work, &["rev-parse", "HEAD"]);
    let tracking_before = git_stdout(&repository.work, &["rev-parse", "origin/master"]);
    fs::write(repository.seed.join("remote.txt"), "remote\n").unwrap();
    git(&repository.seed, &["add", "."]);
    git(&repository.seed, &["commit", "-m", "Remote commit"]);
    git(&repository.seed, &["push", "origin", "HEAD"]);
    let remote_before = remote_head(&repository.seed);

    let result = super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect("repair protected upstream");

    assert!(matches!(
        result,
        OperationResult::Sync { plan }
            if plan.upstream == "origin/work" && plan.establish_upstream
    ));
    assert_eq!(remote_head(&repository.seed), remote_before);
    assert_ne!(tracking_before, remote_before);
    assert_eq!(
        git_stdout(&repository.work, &["rev-parse", "origin/master"]),
        remote_before
    );
    assert_eq!(
        git_stdout(
            &repository.work,
            &["rev-parse", "--abbrev-ref", "@{upstream}"]
        ),
        "origin/work"
    );
    assert_eq!(
        git_stdout(
            &repository.work,
            &["ls-remote", "--refs", "origin", "refs/heads/work"]
        )
        .split_whitespace()
        .next(),
        Some(local_before.as_str())
    );
}

struct ChangeUpstreamThenConfirm(PathBuf);

impl PromptHandler for ChangeUpstreamThenConfirm {
    fn prompt(
        &self,
        _id: PromptId,
        _prompt: GitPrompt,
        _cancellation: &CancellationHandle,
    ) -> PromptAnswer {
        git(
            &self.0,
            &["config", "branch.main.merge", "refs/heads/other"],
        );
        PromptAnswer::Confirm
    }
}

#[test]
fn mismatched_upstream_repair_rejects_a_stale_protected_plan() {
    let repository = sync_repository();
    git(&repository.work, &["branch", "-m", "main"]);
    fs::write(repository.work.join("local.txt"), "local\n").unwrap();
    git(&repository.work, &["add", "."]);
    git(&repository.work, &["commit", "-m", "Local commit"]);
    let master_before = remote_head(&repository.seed);
    let context = RepositoryOperationContext::new(
        Arc::new(ChangeUpstreamThenConfirm(repository.work.clone())),
        CancellationHandle::default(),
    );

    let failure = super::super::GitRepositorySource::with_askpass(&repository.work)
        .apply_with_context(&RepositoryAction::Sync, &context)
        .expect_err("changed upstream must invalidate the plan");

    assert_eq!(failure.kind, FailureKind::RefChanged);
    assert_eq!(remote_head(&repository.seed), master_before);
    assert!(
        Command::new("git")
            .args([
                "ls-remote",
                "--exit-code",
                "--refs",
                "origin",
                "refs/heads/main"
            ])
            .current_dir(&repository.work)
            .output()
            .unwrap()
            .status
            .code()
            .is_some_and(|code| code == 2)
    );
}
