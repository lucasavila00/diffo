use std::{
    fmt::Write as _,
    fs,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use super::{failure::classify_failure, status::parse_status};
use diffo_core::{
    BranchKind, CancellationHandle, ChangeKind, CheckoutTarget, FailureKind, GitPrompt, HeadState,
    OperationOutcome, OperationResult, ProgressHandler, PromptAnswer, PromptHandler, PromptId,
    Repository, RepositoryAction, RepositoryOperationContext, RepositorySource, SyncPlan,
    SyncProgress,
};

mod checkout;
mod create_branch;
mod delete_branch;
mod explorer;
mod sync_tests;

#[test]
fn configured_askpass_uses_the_running_process_image() {
    let source = super::GitRepositorySource::with_askpass(".");
    let expected = PathBuf::from(format!("/proc/{}/exe", std::process::id()));

    assert_eq!(source.askpass_executable(), Some(expected.as_path()));
}

#[test]
fn parses_branch_files_and_upstream() {
    let status = b"# branch.oid abcdef0123456789\0# branch.head feature\0# branch.upstream origin/feature\0# branch.ab +2 -1\x001 M. N... 100644 100644 100644 abc def file.txt\0? notes.txt\0";

    let parsed = parse_status(status).expect("status should parse");

    let files = parsed
        .files
        .iter()
        .map(|file| (&file.state, file.index_status, file.worktree_status))
        .collect::<Vec<_>>();
    insta::assert_debug_snapshot!((&parsed.head, &parsed.upstream, files));
}

#[test]
fn parses_rename_with_old_path() {
    let status = b"# branch.oid abcdef\0# branch.head main\x002 R. N... 100644 100644 100644 abc def R100 new.txt\0old.txt\0";

    let parsed = parse_status(status).expect("status should parse");

    let file = &parsed.files[0];
    insta::assert_debug_snapshot!((&file.state, file.index_status, file.worktree_status));
}

#[test]
fn distinguishes_unborn_and_detached_head() {
    let unborn = parse_status(b"# branch.oid (initial)\0# branch.head main\0")
        .expect("unborn status should parse");
    let detached = parse_status(b"# branch.oid 123456789abcdef\0# branch.head (detached)\0")
        .expect("detached status should parse");
    insta::assert_debug_snapshot!([unborn.head, detached.head]);
}

#[test]
fn remote_checkout_creates_reuses_and_rejects_a_conflicting_local_branch() {
    let root = tempfile::tempdir().expect("test directory");
    git(
        root.path(),
        &["init", "--bare", "--initial-branch=main", "remote.git"],
    );
    git(root.path(), &["clone", "remote.git", "seed"]);
    let seed = root.path().join("seed");
    git(&seed, &["config", "user.name", "Diffo Test"]);
    git(&seed, &["config", "user.email", "diffo@example.invalid"]);
    fs::write(seed.join("file.txt"), "base\n").unwrap();
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-m", "base"]);
    git(&seed, &["push", "-u", "origin", "HEAD"]);
    git(&seed, &["switch", "-c", "feature/nested"]);
    fs::write(seed.join("file.txt"), "feature\n").unwrap();
    git(&seed, &["commit", "-am", "feature"]);
    git(&seed, &["push", "-u", "origin", "HEAD"]);
    git(&seed, &["push", "origin", "HEAD:refs/heads/conflict"]);
    git(root.path(), &["clone", "remote.git", "work"]);
    let work = root.path().join("work");
    let source = super::GitRepositorySource::new(&work);
    let branches = source.branches().unwrap();
    assert!(branches.iter().all(|branch| branch.name != "origin/HEAD"));
    let remote = branches
        .iter()
        .find(|branch| branch.name == "origin/feature/nested")
        .unwrap();
    let action = RepositoryAction::Checkout(Box::new(CheckoutTarget {
        kind: remote.kind,
        full_ref: remote.full_ref.clone(),
        object_id: remote.object_id.clone(),
    }));

    assert!(matches!(
        source.apply(&action),
        Ok(OperationResult::Checkout { branch }) if branch == "feature/nested"
    ));
    assert_eq!(
        source.snapshot().unwrap().upstream.unwrap().name,
        "origin/feature/nested"
    );
    git(&work, &["checkout", "main"]);
    assert!(matches!(
        source.apply(&action),
        Ok(OperationResult::Checkout { branch }) if branch == "feature/nested"
    ));

    git(&work, &["checkout", "main"]);
    git(&work, &["branch", "conflict"]);
    let conflicting_remote = branches
        .iter()
        .find(|branch| branch.name == "origin/conflict")
        .unwrap();
    let failure = source
        .apply(&RepositoryAction::Checkout(Box::new(CheckoutTarget {
            kind: conflicting_remote.kind,
            full_ref: conflicting_remote.full_ref.clone(),
            object_id: conflicting_remote.object_id.clone(),
        })))
        .unwrap_err();

    assert_eq!(failure.kind, FailureKind::BranchConflict);
    assert!(matches!(
        source.snapshot().unwrap().head,
        HeadState::Named { name, .. } if name == "main"
    ));
}

#[test]
fn checkout_rejects_a_ref_that_changed_after_discovery() {
    let repo = test_repository();
    git(repo.path(), &["branch", "topic"]);
    let source = super::GitRepositorySource::new(repo.path());
    let topic = source
        .branches()
        .unwrap()
        .into_iter()
        .find(|branch| branch.name == "topic")
        .unwrap();
    fs::write(repo.path().join("next.txt"), "next\n").unwrap();
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-m", "next"]);
    git(repo.path(), &["branch", "-f", "topic", "HEAD"]);

    let failure = source
        .apply(&RepositoryAction::Checkout(Box::new(CheckoutTarget {
            kind: topic.kind,
            full_ref: topic.full_ref,
            object_id: topic.object_id,
        })))
        .unwrap_err();

    assert_eq!(failure.kind, FailureKind::RefChanged);
    assert!(matches!(
        source.snapshot().unwrap().head,
        HeadState::Named { name, .. } if name == "main"
    ));
}

#[test]
fn checkout_maps_overwritten_local_changes_to_dirty_worktree() {
    let repo = test_repository();
    git(repo.path(), &["switch", "-c", "topic"]);
    fs::write(repo.path().join("tracked.txt"), "topic\n").unwrap();
    git(repo.path(), &["commit", "-am", "topic"]);
    git(repo.path(), &["switch", "main"]);
    let source = super::GitRepositorySource::new(repo.path());
    let topic = source
        .branches()
        .unwrap()
        .into_iter()
        .find(|branch| branch.name == "topic")
        .unwrap();
    fs::write(repo.path().join("tracked.txt"), "local\n").unwrap();

    let failure = source
        .apply(&RepositoryAction::Checkout(Box::new(CheckoutTarget {
            kind: topic.kind,
            full_ref: topic.full_ref,
            object_id: topic.object_id,
        })))
        .unwrap_err();

    assert_eq!(failure.kind, FailureKind::DirtyWorktree);
}

struct CancelPrompts;

impl PromptHandler for CancelPrompts {
    fn prompt(
        &self,
        _id: PromptId,
        _prompt: GitPrompt,
        _cancellation: &CancellationHandle,
    ) -> PromptAnswer {
        PromptAnswer::Cancel
    }
}

struct ConfirmPrompts;

impl PromptHandler for ConfirmPrompts {
    fn prompt(
        &self,
        _id: PromptId,
        _prompt: GitPrompt,
        _cancellation: &CancellationHandle,
    ) -> PromptAnswer {
        PromptAnswer::Confirm
    }
}

#[test]
fn cancelling_real_checkout_blocked_inside_fsmonitor_preserves_head_and_worktree() {
    let repo = test_repository();
    git(repo.path(), &["switch", "-c", "topic"]);
    fs::write(repo.path().join("tracked.txt"), "topic\n").unwrap();
    git(repo.path(), &["commit", "-am", "topic"]);
    git(repo.path(), &["switch", "main"]);
    let source = super::GitRepositorySource::new(repo.path());
    let topic = source
        .branches()
        .unwrap()
        .into_iter()
        .find(|branch| branch.name == "topic")
        .unwrap();
    let gate = repo.path().join("fsmonitor-release");
    assert!(
        Command::new("mkfifo")
            .arg(&gate)
            .status()
            .unwrap()
            .success()
    );
    let marker = repo.path().join("fsmonitor-started");
    let hook = repo.path().join("fsmonitor-hook");
    fs::write(
        &hook,
        format!(
            "#!/bin/sh\nprintf started > '{}'\nIFS= read -r release < '{}'\nprintf 'token\\n'\n",
            marker.display(),
            gate.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    git(
        repo.path(),
        &["config", "core.fsmonitor", hook.to_str().unwrap()],
    );
    let cancellation = CancellationHandle::default();
    let worker_cancellation = cancellation.clone();
    let root = repo.path().to_owned();
    let operation = thread::spawn(move || {
        let source = super::GitRepositorySource::new(root);
        let action = RepositoryAction::Checkout(Box::new(CheckoutTarget {
            kind: topic.kind,
            full_ref: topic.full_ref,
            object_id: topic.object_id,
        }));
        let context = RepositoryOperationContext::new(Arc::new(CancelPrompts), worker_cancellation);
        source.apply_with_context(&action, &context)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !marker.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(marker.exists(), "checkout did not enter the fsmonitor hook");

    cancellation.cancel();
    assert!(matches!(
        operation.join().unwrap(),
        Ok(OperationOutcome::Cancelled)
    ));
    git(repo.path(), &["config", "--unset", "core.fsmonitor"]);
    assert_eq!(
        String::from_utf8_lossy(&fs::read(repo.path().join("tracked.txt")).unwrap()),
        "base\n"
    );
    assert!(matches!(
        super::GitRepositorySource::new(repo.path()).snapshot().unwrap().head,
        HeadState::Named { name, .. } if name == "main"
    ));
}

#[test]
fn stages_and_unstages_a_file() {
    let repo = test_repository();
    fs::write(repo.path().join("new.txt"), "new\n").expect("write file");
    let source = super::GitRepositorySource::new(repo.path());

    source
        .apply(&RepositoryAction::Stage(PathBuf::from("new.txt")))
        .expect("stage file");
    assert!(
        source
            .snapshot()
            .expect("staged snapshot")
            .files
            .iter()
            .any(|file| file.path == Path::new("new.txt") && file.staged.is_some())
    );

    source
        .apply(&RepositoryAction::Unstage(PathBuf::from("new.txt")))
        .expect("unstage file");
    let file = source
        .snapshot()
        .expect("unstaged snapshot")
        .files
        .into_iter()
        .find(|file| file.path == Path::new("new.txt"))
        .expect("new file");
    assert_eq!(file.kind, ChangeKind::Untracked);
    assert!(file.staged.is_none());
    assert_eq!(
        file.unstaged.expect("untracked diff").text,
        "@@ -0,0 +1,1 @@\n+new\n"
    );
}

#[test]
fn stages_and_unstages_all_files() {
    let repo = test_repository();
    fs::write(repo.path().join("tracked.txt"), "changed\n").expect("modify file");
    fs::write(repo.path().join("new.txt"), "new\n").expect("write file");
    let source = super::GitRepositorySource::new(repo.path());

    source
        .apply(&RepositoryAction::StageAll)
        .expect("stage all files");
    let snapshot = source.snapshot().expect("snapshot");

    assert_eq!(snapshot.files.len(), 2);
    assert!(snapshot.files.iter().all(|file| file.staged.is_some()));

    source
        .apply(&RepositoryAction::UnstageAll)
        .expect("unstage all files");
    let snapshot = source.snapshot().expect("unstaged snapshot");
    assert_eq!(snapshot.files.len(), 2);
    assert!(snapshot.files.iter().all(|file| file.staged.is_none()));
    assert!(snapshot.files.iter().all(|file| file.unstaged.is_some()));
}

#[test]
fn snapshots_the_whole_untracked_file_as_an_addition() {
    let repo = test_repository();
    fs::write(repo.path().join("new.txt"), "first\nsecond").expect("write file");
    let source = super::GitRepositorySource::new(repo.path());

    let diff = source
        .snapshot()
        .expect("snapshot")
        .files
        .into_iter()
        .find(|file| file.path == Path::new("new.txt"))
        .and_then(|file| file.unstaged)
        .expect("untracked diff");

    assert_eq!(
        diff.text,
        "@@ -0,0 +1,2 @@\n+first\n+second\n\\ No newline at end of file\n"
    );
}

#[test]
fn snapshots_the_whole_modified_file_as_diff_context() {
    let repo = test_repository();
    let mut original = String::new();
    for line in 1..=20 {
        writeln!(original, "line {line}").expect("write test contents");
    }
    fs::write(repo.path().join("long.txt"), &original).expect("write original file");
    git(repo.path(), &["add", "long.txt"]);
    git(repo.path(), &["commit", "-m", "Add long file"]);
    let changed = original.replace("line 10\n", "changed 10\n");
    fs::write(repo.path().join("long.txt"), changed).expect("change file");

    let diff = super::GitRepositorySource::new(repo.path())
        .snapshot()
        .expect("snapshot")
        .files
        .into_iter()
        .find(|file| file.path == Path::new("long.txt"))
        .and_then(|file| file.unstaged)
        .expect("modified diff");

    assert!(diff.text.contains(" line 1\n"));
    assert!(diff.text.contains(" line 20\n"));
    assert!(diff.text.contains("-line 10\n+changed 10\n"));
}

#[test]
fn sync_fast_forwards_to_the_refreshed_upstream() {
    let root = tempfile::tempdir().expect("test directory");
    git(root.path(), &["init", "--bare", "remote.git"]);
    git(root.path(), &["clone", "remote.git", "seed"]);
    let seed = root.path().join("seed");
    git(&seed, &["config", "user.name", "Diffo Test"]);
    git(&seed, &["config", "user.email", "diffo@example.invalid"]);
    fs::write(seed.join("base.txt"), "base\n").expect("write base file");
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-m", "Base commit"]);
    git(&seed, &["push", "-u", "origin", "HEAD"]);
    git(root.path(), &["clone", "remote.git", "work"]);
    let work = root.path().join("work");

    fs::write(seed.join("remote.txt"), "remote\n").expect("write remote file");
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-m", "Remote commit"]);
    git(&seed, &["push", "origin", "HEAD"]);

    let source = super::GitRepositorySource::new(&work);
    let result = source.apply(&RepositoryAction::Sync).expect("sync remote");
    assert_eq!(
        result,
        OperationResult::Sync {
            plan: Box::new(SyncPlan {
                branch: "master".to_owned(),
                upstream: "origin/master".to_owned(),
                local_only: 0,
                upstream_only: 1,
            }),
        }
    );
    assert!(work.join("remote.txt").exists());
    assert_eq!(
        source
            .snapshot()
            .expect("synced snapshot")
            .upstream
            .unwrap()
            .behind,
        0
    );
}

#[test]
fn sync_cannot_push_a_protected_branch_without_confirmation_context() {
    let repository = sync_repository();
    fs::write(repository.work.join("local.txt"), "local\n").unwrap();
    git(&repository.work, &["add", "."]);
    git(&repository.work, &["commit", "-m", "Local commit"]);
    let remote_before = remote_head(&repository.seed);

    let failure = super::GitRepositorySource::new(&repository.work)
        .apply(&RepositoryAction::Sync)
        .expect_err("protected push without confirmation must stop");

    assert_eq!(failure.kind, FailureKind::Unknown);
    assert_eq!(
        failure.detail,
        "protected branch push confirmation is unavailable"
    );
    assert_eq!(remote_head(&repository.seed), remote_before);
}

#[test]
fn sync_pushes_local_only_commits_after_fetching() {
    let repository = sync_repository();
    fs::write(repository.work.join("local.txt"), "local\n").unwrap();
    git(&repository.work, &["add", "."]);
    git(&repository.work, &["commit", "-m", "Local commit"]);

    let result = confirmed_sync(&repository.work).expect("sync local commit");

    assert!(matches!(
        result,
        OperationOutcome::Completed(OperationResult::Sync { plan })
            if plan.local_only == 1 && plan.upstream_only == 0
    ));
    assert_eq!(
        git_stdout(&repository.work, &["rev-parse", "HEAD"]),
        remote_head(&repository.seed)
    );
}

#[test]
fn sync_rebases_clean_divergence_and_pushes_without_a_merge_commit() {
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

    let result = confirmed_sync(&repository.work).expect("sync divergent commits");
    let new_local = git_stdout(&repository.work, &["rev-parse", "HEAD"]);

    assert!(matches!(
        result,
        OperationOutcome::Completed(OperationResult::Sync { plan })
            if plan.local_only == 1 && plan.upstream_only == 1
    ));
    assert_ne!(
        new_local, old_local,
        "rebase must give the local commit a new ID"
    );
    assert!(repository.work.join("local.txt").exists());
    assert!(repository.work.join("remote.txt").exists());
    assert_eq!(
        git_stdout(&repository.work, &["log", "-1", "--format=%s"]),
        "Local commit"
    );
    assert_eq!(
        git_stdout(
            &repository.work,
            &["rev-list", "--merges", &format!("{remote}..HEAD")]
        ),
        ""
    );
    assert_eq!(new_local, remote_head(&repository.seed));
}

#[test]
fn classifies_failures_without_returning_git_secrets() {
    for (action, output, expected) in [
        (
            RepositoryAction::Sync,
            "[rejected] (non-fast-forward)",
            FailureKind::PushRejected,
        ),
        (
            RepositoryAction::Sync,
            "pre-receive hook declined: token=secret",
            FailureKind::HookRejected,
        ),
        (
            RepositoryAction::Sync,
            "CONFLICT in file",
            FailureKind::RebaseConflict,
        ),
        (
            RepositoryAction::Fetch,
            "fatal: could not resolve host",
            FailureKind::Network,
        ),
    ] {
        let failure = classify_failure(&action, output);
        assert_eq!(failure.kind, expected);
        assert!(!failure.detail.contains("secret"));
    }
}

struct SyncRepository {
    root: tempfile::TempDir,
    seed: PathBuf,
    work: PathBuf,
}

fn sync_repository() -> SyncRepository {
    let root = tempfile::tempdir().expect("sync test directory");
    git(root.path(), &["init", "--bare", "remote.git"]);
    git(root.path(), &["clone", "remote.git", "seed"]);
    let seed = root.path().join("seed");
    git(&seed, &["config", "user.name", "Diffo Test"]);
    git(&seed, &["config", "user.email", "diffo@example.invalid"]);
    fs::write(seed.join("base.txt"), "base\n").unwrap();
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-m", "Base commit"]);
    git(&seed, &["push", "-u", "origin", "HEAD"]);
    git(root.path(), &["clone", "remote.git", "work"]);
    let work = root.path().join("work");
    git(&work, &["config", "user.name", "Diffo Test"]);
    git(&work, &["config", "user.email", "diffo@example.invalid"]);
    SyncRepository { root, seed, work }
}

fn confirmed_sync(
    repository: &Path,
) -> std::result::Result<OperationOutcome, diffo_core::OperationFailure> {
    let context =
        RepositoryOperationContext::new(Arc::new(ConfirmPrompts), CancellationHandle::default());
    super::GitRepositorySource::with_askpass(repository)
        .apply_with_context(&RepositoryAction::Sync, &context)
}

fn git_stdout(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run git for output");
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn remote_head(repo: &Path) -> String {
    git_stdout(repo, &["ls-remote", "origin", "HEAD"])
        .split_whitespace()
        .next()
        .expect("remote HEAD hash")
        .to_owned()
}

fn test_repository() -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("test directory");
    git(repo.path(), &["init", "--initial-branch=main"]);
    git(repo.path(), &["config", "user.name", "Diffo Test"]);
    git(
        repo.path(),
        &["config", "user.email", "diffo@example.invalid"],
    );
    fs::write(repo.path().join("tracked.txt"), "base\n").expect("write tracked file");
    git(repo.path(), &["add", "tracked.txt"]);
    git(repo.path(), &["commit", "-m", "Base commit"]);
    repo
}

fn git(repo: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run git");
    assert!(
        status.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );
}
