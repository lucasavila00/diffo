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
fn sync_without_an_upstream_stops_when_no_remote_exists() {
    let repository = test_repository();
    let head = git_stdout(repository.path(), &["rev-parse", "HEAD"]);

    let failure = super::super::GitRepositorySource::new(repository.path())
        .apply(&RepositoryAction::Sync)
        .expect_err("sync without a remote must stop");

    assert_eq!(failure.kind, FailureKind::NoRemote);
    assert_eq!(git_stdout(repository.path(), &["rev-parse", "HEAD"]), head);
}

#[test]
fn sync_repairs_a_missing_upstream_when_the_remote_tip_is_equal() {
    let repository = sync_repository();
    git(&repository.work, &["branch", "--unset-upstream"]);

    let result = super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect("sync missing upstream");

    assert!(matches!(
        result,
        OperationResult::Sync { plan }
            if plan.local_only == 0
                && plan.upstream_only == 0
                && plan.establish_upstream
    ));
    assert_eq!(
        git_stdout(
            &repository.work,
            &["rev-parse", "--abbrev-ref", "@{upstream}"]
        ),
        "origin/master"
    );
}

#[test]
fn sync_publishes_a_new_same_named_branch_and_sets_its_upstream() {
    let repository = sync_repository();
    git(&repository.work, &["switch", "-c", "topic"]);
    fs::write(repository.work.join("topic.txt"), "topic\n").unwrap();
    git(&repository.work, &["add", "."]);
    git(&repository.work, &["commit", "-m", "Topic commit"]);

    let result = super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect("publish with sync");

    assert!(matches!(
        result,
        OperationResult::Sync { plan }
            if plan.branch == "topic" && plan.upstream == "origin/topic"
                && plan.establish_upstream
    ));
    assert_eq!(
        git_stdout(
            &repository.work,
            &["rev-parse", "--abbrev-ref", "@{upstream}"]
        ),
        "origin/topic"
    );
    assert_eq!(
        git_stdout(&repository.work, &["rev-parse", "HEAD"]),
        git_stdout(
            &repository.work,
            &["ls-remote", "--refs", "origin", "refs/heads/topic"]
        )
        .split_whitespace()
        .next()
        .unwrap()
    );
}

#[test]
fn sync_fast_forwards_an_existing_destination_before_setting_upstream() {
    let repository = sync_repository();
    git(&repository.work, &["branch", "--unset-upstream"]);
    fs::write(repository.seed.join("remote.txt"), "remote\n").unwrap();
    git(&repository.seed, &["add", "."]);
    git(&repository.seed, &["commit", "-m", "Remote commit"]);
    git(&repository.seed, &["push", "origin", "HEAD"]);
    let remote = git_stdout(&repository.seed, &["rev-parse", "HEAD"]);

    let result = super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect("sync existing destination");

    assert!(matches!(
        result,
        OperationResult::Sync { plan }
            if plan.upstream_only == 1 && plan.establish_upstream
    ));
    assert_eq!(git_stdout(&repository.work, &["rev-parse", "HEAD"]), remote);
    assert_eq!(
        git_stdout(
            &repository.work,
            &["rev-parse", "--abbrev-ref", "@{upstream}"]
        ),
        "origin/master"
    );
}

#[test]
fn sync_uses_the_only_non_origin_remote_for_a_missing_upstream() {
    let repository = sync_repository();
    git(
        &repository.work,
        &["remote", "rename", "origin", "upstream"],
    );
    git(&repository.work, &["branch", "--unset-upstream"]);

    super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect("sync sole remote");

    assert_eq!(
        git_stdout(
            &repository.work,
            &["rev-parse", "--abbrev-ref", "@{upstream}"]
        ),
        "upstream/master"
    );
}

#[test]
fn sync_prefers_origin_when_a_missing_upstream_has_several_remotes() {
    let repository = sync_repository();
    let remote_path = repository.root.path().join("remote.git");
    git(&repository.work, &["branch", "--unset-upstream"]);
    git(
        &repository.work,
        &["remote", "add", "backup", remote_path.to_str().unwrap()],
    );

    super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect("prefer origin");

    assert_eq!(
        git_stdout(
            &repository.work,
            &["rev-parse", "--abbrev-ref", "@{upstream}"]
        ),
        "origin/master"
    );
}

#[test]
fn sync_requires_a_remote_choice_only_when_origin_is_absent_and_several_exist() {
    let repository = sync_repository();
    let remote_path = repository.root.path().join("remote.git");
    git(&repository.work, &["branch", "--unset-upstream"]);
    git(&repository.work, &["remote", "rename", "origin", "alpha"]);
    git(
        &repository.work,
        &["remote", "add", "beta", remote_path.to_str().unwrap()],
    );

    let failure = super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect_err("ambiguous remote must stop");
    assert_eq!(failure.kind, FailureKind::NoRemote);

    super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::SyncToRemote("beta".to_owned()))
        .expect("selected remote");
    assert_eq!(
        git_stdout(
            &repository.work,
            &["rev-parse", "--abbrev-ref", "@{upstream}"]
        ),
        "beta/master"
    );
}

#[test]
fn sync_rejects_an_existing_same_named_branch_with_unrelated_history() {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "--bare", "remote.git"]);
    git(root.path(), &["clone", "remote.git", "seed"]);
    let seed = root.path().join("seed");
    git(&seed, &["config", "user.name", "Diffo Test"]);
    git(&seed, &["config", "user.email", "diffo@example.invalid"]);
    fs::write(seed.join("remote.txt"), "remote\n").unwrap();
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-m", "Remote root"]);
    git(&seed, &["push", "origin", "HEAD:refs/heads/main"]);
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
    let local = git_stdout(repository.path(), &["rev-parse", "HEAD"]);

    let failure = super::super::GitRepositorySource::new(repository.path())
        .apply(&RepositoryAction::Sync)
        .expect_err("unrelated destination must stop");

    assert_eq!(failure.kind, FailureKind::BranchConflict);
    assert_eq!(git_stdout(repository.path(), &["rev-parse", "HEAD"]), local);
    assert!(
        !Command::new("git")
            .args(["rev-parse", "--verify", "@{upstream}"])
            .current_dir(repository.path())
            .output()
            .unwrap()
            .status
            .success()
    );
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

    let failure = confirmed_sync(&repository.work).expect_err("merge history must stop sync");

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

    let failure = confirmed_sync(&repository.work).expect_err("hook must reject the push");
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

    let failure = confirmed_sync(&repository.work).expect_err("conflicting sync must stop");

    assert_eq!(failure.kind, FailureKind::RebaseConflict);
    assert!(
        failure
            .detail
            .starts_with("Rebase conflicted in 1 file and was aborted. Nothing was pushed.")
    );
    assert!(failure.detail.contains("Git exit status: 1"));
    assert!(failure.detail.contains("stderr:"));
    assert!(failure.detail.contains("stdout:"));
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

    confirmed_sync(&repository.work).expect("sync separate hunks");

    let combined = fs::read_to_string(repository.work.join("shared.txt")).unwrap();
    assert!(combined.contains("local 5\n"));
    assert!(combined.contains("remote 15\n"));
}

#[test]
fn sync_fast_forwards_with_unrelated_staged_unstaged_and_untracked_work() {
    let repository = sync_repository();
    fs::write(repository.seed.join("remote.txt"), "remote\n").unwrap();
    git(&repository.seed, &["add", "."]);
    git(&repository.seed, &["commit", "-m", "Remote commit"]);
    git(&repository.seed, &["push", "origin", "HEAD"]);
    fs::write(repository.work.join("base.txt"), "dirty\n").unwrap();
    fs::write(repository.work.join("staged.txt"), "staged\n").unwrap();
    git(&repository.work, &["add", "staged.txt"]);
    fs::write(repository.work.join("untracked.txt"), "untracked\n").unwrap();
    let status = git_stdout(&repository.work, &["status", "--porcelain=v1"]);

    super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect("dirty fast-forward");

    assert_eq!(
        git_stdout(&repository.work, &["status", "--porcelain=v1"]),
        status
    );
    assert_eq!(
        fs::read_to_string(repository.work.join("remote.txt")).unwrap(),
        "remote\n"
    );
}

#[test]
fn sync_pushes_commits_without_touching_uncommitted_work() {
    let repository = sync_repository();
    fs::write(repository.work.join("local.txt"), "committed\n").unwrap();
    git(&repository.work, &["add", "."]);
    git(&repository.work, &["commit", "-m", "Local commit"]);
    fs::write(repository.work.join("base.txt"), "unstaged\n").unwrap();
    fs::write(repository.work.join("staged.txt"), "staged\n").unwrap();
    git(&repository.work, &["add", "staged.txt"]);
    fs::write(repository.work.join("untracked.txt"), "untracked\n").unwrap();
    let status = git_stdout(&repository.work, &["status", "--porcelain=v1"]);

    confirmed_sync(&repository.work).expect("dirty push");

    assert_eq!(
        git_stdout(&repository.work, &["status", "--porcelain=v1"]),
        status
    );
    assert_eq!(
        git_stdout(&repository.work, &["rev-parse", "HEAD"]),
        remote_head(&repository.seed)
    );
}

#[test]
fn sync_preserves_overlapping_tracked_work_when_fast_forward_is_refused() {
    let repository = sync_repository();
    let local_head = git_stdout(&repository.work, &["rev-parse", "HEAD"]);
    fs::write(repository.work.join("base.txt"), "local\n").unwrap();
    fs::write(repository.seed.join("base.txt"), "remote\n").unwrap();
    git(&repository.seed, &["commit", "-am", "Remote conflict"]);
    git(&repository.seed, &["push", "origin", "HEAD"]);
    let remote = git_stdout(&repository.seed, &["rev-parse", "HEAD"]);

    let failure = super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect_err("overlapping fast-forward must stop");

    assert_eq!(failure.kind, FailureKind::DirtyWorktree);
    assert_eq!(
        git_stdout(&repository.work, &["rev-parse", "HEAD"]),
        local_head
    );
    assert_eq!(
        git_stdout(&repository.work, &["rev-parse", "origin/master"]),
        remote
    );
    assert_eq!(
        fs::read_to_string(repository.work.join("base.txt")).unwrap(),
        "local\n"
    );
}

#[test]
fn sync_preserves_an_untracked_file_that_blocks_fast_forward() {
    let repository = sync_repository();
    let local_head = git_stdout(&repository.work, &["rev-parse", "HEAD"]);
    fs::write(repository.work.join("collision.txt"), "local\n").unwrap();
    fs::write(repository.seed.join("collision.txt"), "remote\n").unwrap();
    git(&repository.seed, &["add", "."]);
    git(&repository.seed, &["commit", "-m", "Remote file"]);
    git(&repository.seed, &["push", "origin", "HEAD"]);

    let failure = super::super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect_err("untracked collision must stop");

    assert_eq!(failure.kind, FailureKind::DirtyWorktree);
    assert_eq!(
        git_stdout(&repository.work, &["rev-parse", "HEAD"]),
        local_head
    );
    assert_eq!(
        fs::read_to_string(repository.work.join("collision.txt")).unwrap(),
        "local\n"
    );
}

#[test]
fn sync_rebases_with_an_unrelated_untracked_file() {
    let repository = sync_repository();
    fs::write(repository.work.join("local.txt"), "local\n").unwrap();
    git(&repository.work, &["add", "."]);
    git(&repository.work, &["commit", "-m", "Local commit"]);
    fs::write(repository.work.join("unfinished.txt"), "unfinished\n").unwrap();
    fs::write(repository.seed.join("remote.txt"), "remote\n").unwrap();
    git(&repository.seed, &["add", "."]);
    git(&repository.seed, &["commit", "-m", "Remote commit"]);
    git(&repository.seed, &["push", "origin", "HEAD"]);

    confirmed_sync(&repository.work).expect("rebase with untracked work");

    assert_eq!(
        fs::read_to_string(repository.work.join("unfinished.txt")).unwrap(),
        "unfinished\n"
    );
    assert_eq!(
        git_stdout(&repository.work, &["rev-parse", "HEAD"]),
        remote_head(&repository.seed)
    );
}

#[test]
fn sync_fetches_then_stops_when_dirty_tracked_work_blocks_a_rebase() {
    let repository = sync_repository();
    fs::write(repository.work.join("local.txt"), "local\n").unwrap();
    git(&repository.work, &["add", "."]);
    git(&repository.work, &["commit", "-m", "Local commit"]);
    fs::write(repository.work.join("base.txt"), "dirty\n").unwrap();
    let local = git_stdout(&repository.work, &["rev-parse", "HEAD"]);
    fs::write(repository.seed.join("remote.txt"), "remote\n").unwrap();
    git(&repository.seed, &["add", "."]);
    git(&repository.seed, &["commit", "-m", "Remote commit"]);
    git(&repository.seed, &["push", "origin", "HEAD"]);
    let remote = git_stdout(&repository.seed, &["rev-parse", "HEAD"]);

    let failure = confirmed_sync(&repository.work).expect_err("dirty rebase must stop");

    assert_eq!(failure.kind, FailureKind::DirtyWorktree);
    assert_eq!(git_stdout(&repository.work, &["rev-parse", "HEAD"]), local);
    assert_eq!(
        git_stdout(&repository.work, &["rev-parse", "origin/master"]),
        remote
    );
    assert_eq!(
        fs::read_to_string(repository.work.join("base.txt")).unwrap(),
        "dirty\n"
    );
    assert_eq!(remote_head(&repository.seed), remote);
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
        Arc::new(ConfirmPrompts),
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

struct MoveHeadThenConfirm(PathBuf);

impl PromptHandler for MoveHeadThenConfirm {
    fn prompt(
        &self,
        _id: PromptId,
        _prompt: GitPrompt,
        _cancellation: &CancellationHandle,
    ) -> PromptAnswer {
        git(
            &self.0,
            &["commit", "--allow-empty", "-m", "Concurrent commit"],
        );
        PromptAnswer::Confirm
    }
}

#[test]
fn protected_push_confirmation_rejects_a_changed_plan() {
    let repository = sync_repository();
    fs::write(repository.work.join("local.txt"), "local\n").unwrap();
    git(&repository.work, &["add", "."]);
    git(&repository.work, &["commit", "-m", "Local commit"]);
    let remote_before = remote_head(&repository.seed);
    let context = RepositoryOperationContext::new(
        Arc::new(MoveHeadThenConfirm(repository.work.clone())),
        CancellationHandle::default(),
    );

    let failure = super::super::GitRepositorySource::with_askpass(&repository.work)
        .apply_with_context(&RepositoryAction::Sync, &context)
        .expect_err("changed plan must stop");

    assert_eq!(failure.kind, FailureKind::RefChanged);
    assert_eq!(
        failure.detail,
        "repository state changed while confirming the push; start Sync again"
    );
    assert_eq!(remote_head(&repository.seed), remote_before);
}
