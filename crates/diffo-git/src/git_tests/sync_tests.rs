use super::*;

#[test]
fn sync_fetches_even_when_the_known_tips_are_the_same() {
    let repository = sync_repository();
    let marker = repository.root.path().join("fetch-ran");
    let upload_pack = repository.root.path().join("upload-pack");
    fs::write(
        &upload_pack,
        format!(
            "#!/bin/sh\ntouch '{}'\nexec git-upload-pack \"$@\"\n",
            marker.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&upload_pack, fs::Permissions::from_mode(0o755)).unwrap();
    git(
        &repository.work,
        &[
            "config",
            "remote.origin.uploadpack",
            upload_pack.to_str().unwrap(),
        ],
    );

    let result = super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect("sync equal tips");

    assert!(marker.exists());
    assert!(matches!(
        result,
        OperationResult::Sync { plan }
            if plan.local_only == 0 && plan.upstream_only == 0
    ));
}

#[test]
fn sync_stops_before_fetch_without_an_upstream() {
    let repository = test_repository();
    let head = git_stdout(repository.path(), &["rev-parse", "HEAD"]);

    let failure = super::super::GitRepositorySource::new(repository.path())
        .apply(&RepositoryAction::Sync)
        .expect_err("sync without upstream must stop");

    assert_eq!(failure.kind, FailureKind::NoUpstream);
    assert_eq!(git_stdout(repository.path(), &["rev-parse", "HEAD"]), head);
}

#[test]
fn sync_rejects_local_merge_commits_before_rebase_or_push() {
    let repository = sync_repository();
    git(&repository.work, &["switch", "-c", "topic"]);
    fs::write(repository.work.join("topic.txt"), "topic\n").unwrap();
    git(&repository.work, &["add", "."]);
    git(&repository.work, &["commit", "-m", "Topic commit"]);
    git(&repository.work, &["switch", "master"]);
    fs::write(repository.work.join("main.txt"), "main\n").unwrap();
    git(&repository.work, &["add", "."]);
    git(&repository.work, &["commit", "-m", "Main commit"]);
    git(
        &repository.work,
        &["merge", "--no-ff", "topic", "-m", "Local merge"],
    );
    let old_local = git_stdout(&repository.work, &["rev-parse", "HEAD"]);
    fs::write(repository.seed.join("remote.txt"), "remote\n").unwrap();
    git(&repository.seed, &["add", "."]);
    git(&repository.seed, &["commit", "-m", "Remote commit"]);
    git(&repository.seed, &["push", "origin", "HEAD"]);
    let remote = git_stdout(&repository.seed, &["rev-parse", "HEAD"]);

    let failure = super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect_err("merge history must stop sync");

    assert_eq!(failure.kind, FailureKind::MergeCommits);
    assert_eq!(
        git_stdout(&repository.work, &["rev-parse", "HEAD"]),
        old_local
    );
    assert_eq!(remote_head(&repository.seed), remote);
}

#[test]
fn rejected_push_after_rebase_leaves_the_rebased_local_commits() {
    let repository = sync_repository();
    fs::write(repository.work.join("local.txt"), "local\n").unwrap();
    git(&repository.work, &["add", "."]);
    git(&repository.work, &["commit", "-m", "Local commit"]);
    let old_local = git_stdout(&repository.work, &["rev-parse", "HEAD"]);
    fs::write(repository.seed.join("remote.txt"), "remote\n").unwrap();
    git(&repository.seed, &["add", "."]);
    git(&repository.seed, &["commit", "-m", "Remote commit"]);
    git(&repository.seed, &["push", "origin", "HEAD"]);
    let remote = git_stdout(&repository.seed, &["rev-parse", "HEAD"]);
    let hook = repository.root.path().join("remote.git/hooks/pre-receive");
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();

    let failure = super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect_err("hook must reject the push");
    let new_local = git_stdout(&repository.work, &["rev-parse", "HEAD"]);

    assert_eq!(failure.kind, FailureKind::HookRejected);
    assert_ne!(new_local, old_local);
    assert_eq!(remote_head(&repository.seed), remote);
    assert!(repository.work.join("local.txt").exists());
    assert!(repository.work.join("remote.txt").exists());
}

#[test]
fn conflicting_sync_aborts_rebase_and_does_not_push() {
    let repository = sync_repository();
    fs::write(repository.work.join("base.txt"), "local\n").unwrap();
    git(&repository.work, &["commit", "-am", "Local conflict"]);
    let old_local = git_stdout(&repository.work, &["rev-parse", "HEAD"]);
    fs::write(repository.seed.join("base.txt"), "remote\n").unwrap();
    git(&repository.seed, &["commit", "-am", "Remote conflict"]);
    git(&repository.seed, &["push", "origin", "HEAD"]);
    let remote = git_stdout(&repository.seed, &["rev-parse", "HEAD"]);

    let failure = super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect_err("conflicting sync must stop");

    assert_eq!(failure.kind, FailureKind::RebaseConflict);
    assert_eq!(
        failure.detail,
        "Rebase conflicted in 1 file and was aborted. Nothing was pushed."
    );
    assert_eq!(
        git_stdout(&repository.work, &["rev-parse", "HEAD"]),
        old_local
    );
    assert_eq!(remote_head(&repository.seed), remote);
    assert!(!repository.work.join(".git/rebase-merge").exists());
    assert!(!repository.work.join(".git/rebase-apply").exists());
}

#[test]
fn sync_rebases_non_overlapping_hunks_in_one_file() {
    let repository = sync_repository();
    let mut lines = String::new();
    for line in 1..=20 {
        writeln!(lines, "line {line}").unwrap();
    }
    fs::write(repository.seed.join("shared.txt"), &lines).unwrap();
    git(&repository.seed, &["add", "."]);
    git(&repository.seed, &["commit", "-m", "Shared base"]);
    git(&repository.seed, &["push", "origin", "HEAD"]);
    git(&repository.work, &["pull", "--ff-only"]);
    fs::write(
        repository.work.join("shared.txt"),
        lines.replace("line 5\n", "local 5\n"),
    )
    .unwrap();
    git(&repository.work, &["commit", "-am", "Local hunk"]);
    fs::write(
        repository.seed.join("shared.txt"),
        lines.replace("line 15\n", "remote 15\n"),
    )
    .unwrap();
    git(&repository.seed, &["commit", "-am", "Remote hunk"]);
    git(&repository.seed, &["push", "origin", "HEAD"]);

    super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect("sync separate hunks");

    let combined = fs::read_to_string(repository.work.join("shared.txt")).unwrap();
    assert!(combined.contains("local 5\n"));
    assert!(combined.contains("remote 15\n"));
}

#[test]
fn sync_stops_before_fetch_when_the_worktree_is_dirty() {
    let repository = sync_repository();
    fs::write(repository.seed.join("remote.txt"), "remote\n").unwrap();
    git(&repository.seed, &["add", "."]);
    git(&repository.seed, &["commit", "-m", "Remote commit"]);
    git(&repository.seed, &["push", "origin", "HEAD"]);
    let before_fetch = git_stdout(&repository.work, &["rev-parse", "origin/HEAD"]);
    fs::write(repository.work.join("base.txt"), "dirty\n").unwrap();

    let failure = super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect_err("dirty sync must stop");

    assert_eq!(failure.kind, FailureKind::DirtyWorktree);
    assert_eq!(
        git_stdout(&repository.work, &["rev-parse", "origin/HEAD"]),
        before_fetch
    );
    assert_eq!(
        fs::read_to_string(repository.work.join("base.txt")).unwrap(),
        "dirty\n"
    );
}

#[derive(Default)]
struct RecordedProgress(Mutex<Vec<SyncProgress>>);

impl ProgressHandler for RecordedProgress {
    fn progress(&self, progress: SyncProgress) {
        self.0.lock().unwrap().push(progress);
    }
}

#[test]
fn sync_reports_the_plan_before_the_operations_it_runs() {
    let repository = sync_repository();
    fs::write(repository.work.join("local.txt"), "local\n").unwrap();
    git(&repository.work, &["add", "."]);
    git(&repository.work, &["commit", "-m", "Local commit"]);
    fs::write(repository.seed.join("remote.txt"), "remote\n").unwrap();
    git(&repository.seed, &["add", "."]);
    git(&repository.seed, &["commit", "-m", "Remote commit"]);
    git(&repository.seed, &["push", "origin", "HEAD"]);
    let progress = Arc::new(RecordedProgress::default());
    let context = RepositoryOperationContext::with_progress(
        Arc::new(CancelPrompts),
        CancellationHandle::default(),
        Arc::clone(&progress) as Arc<dyn ProgressHandler>,
    );

    super::super::GitRepositorySource::with_askpass(&repository.work)
        .apply_with_context(&RepositoryAction::Sync, &context)
        .expect("sync with progress");

    let progress = progress.0.lock().unwrap();
    assert!(matches!(
        progress.as_slice(),
        [
            SyncProgress::Fetching,
            SyncProgress::Plan(SyncPlan {
                local_only: 1,
                upstream_only: 1,
                ..
            }),
            SyncProgress::Rebasing { commits: 1 },
            SyncProgress::Pushing,
        ]
    ));
}
