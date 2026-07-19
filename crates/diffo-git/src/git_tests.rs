use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use super::{operation::classify_failure, status::parse_status};
use diffo_core::{
    ChangeKind, ExplorerFileContent, FailureKind, OperationResult, Repository, RepositoryAction,
    RepositorySource,
};

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
fn fetches_and_pulls_from_the_configured_remote() {
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
    let fetch = source
        .apply(&RepositoryAction::Fetch)
        .expect("fetch remote");
    assert_eq!(fetch, OperationResult::Fetch { updated_refs: 1 });
    assert_eq!(
        source
            .snapshot()
            .expect("fetched snapshot")
            .upstream
            .unwrap()
            .behind,
        1
    );

    let pull = source.apply(&RepositoryAction::Pull).expect("pull remote");
    assert_eq!(pull, OperationResult::Pull { commits: 1 });
    assert!(work.join("remote.txt").exists());
    assert_eq!(
        source
            .snapshot()
            .expect("pulled snapshot")
            .upstream
            .unwrap()
            .behind,
        0
    );
}

#[test]
fn explorer_lists_unchanged_and_untracked_but_not_ignored_paths() {
    let repo = test_repository();
    fs::write(repo.path().join("untracked.txt"), "new\n").expect("write untracked file");
    fs::write(repo.path().join("removed.txt"), "remove\n").expect("write removable file");
    git(repo.path(), &["add", "removed.txt"]);
    git(repo.path(), &["commit", "-m", "add removable file"]);
    fs::remove_file(repo.path().join("removed.txt")).expect("remove tracked file");
    fs::write(repo.path().join("ignored.txt"), "ignored\n").expect("write ignored file");
    fs::write(repo.path().join(".gitignore"), "ignored.txt\n").expect("write ignore file");
    let source = super::GitRepositorySource::new(repo.path());

    let paths = source.explorer_paths().expect("Explorer paths");

    assert!(paths.contains(&PathBuf::from("tracked.txt")));
    assert!(paths.contains(&PathBuf::from("untracked.txt")));
    assert!(paths.contains(&PathBuf::from(".gitignore")));
    assert!(!paths.contains(&PathBuf::from("ignored.txt")));
    assert!(!paths.contains(&PathBuf::from("removed.txt")));
}

#[test]
fn explorer_reads_worktree_contents_and_rejects_removed_paths() {
    let repo = test_repository();
    let source = super::GitRepositorySource::new(repo.path());
    fs::write(repo.path().join("tracked.txt"), "changed\n").expect("modify file");

    let changed = source
        .explorer_file(Path::new("tracked.txt"))
        .expect("changed file");
    assert_eq!(
        changed.content,
        ExplorerFileContent::Text("changed\n".to_owned())
    );
    assert!(changed.patch.contains("+changed"));

    fs::remove_file(repo.path().join("tracked.txt")).expect("delete file");
    let error = source
        .explorer_file(Path::new("tracked.txt"))
        .expect_err("removed file must not be readable in Explorer");
    assert!(error.to_string().contains("failed to inspect file"));
}

#[test]
fn explorer_marks_binary_contents_without_decoding_them() {
    let repo = test_repository();
    fs::write(repo.path().join("binary.dat"), [0, 159, 146, 150]).expect("write binary file");
    let source = super::GitRepositorySource::new(repo.path());

    let file = source
        .explorer_file(Path::new("binary.dat"))
        .expect("binary file");

    assert_eq!(file.content, ExplorerFileContent::Binary);
}

#[test]
fn classifies_failures_without_returning_git_secrets() {
    for (action, output, expected) in [
        (
            RepositoryAction::Push,
            "[rejected] (non-fast-forward)",
            FailureKind::PushRejected,
        ),
        (
            RepositoryAction::Push,
            "pre-receive hook declined: token=secret",
            FailureKind::HookRejected,
        ),
        (
            RepositoryAction::Pull,
            "CONFLICT in file",
            FailureKind::MergeConflict,
        ),
        (
            RepositoryAction::Pull,
            "fatal: Not possible to fast-forward, aborting.",
            FailureKind::Diverged,
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
