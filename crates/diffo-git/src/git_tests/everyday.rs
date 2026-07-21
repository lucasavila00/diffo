use std::{fs, path::Path};

use diffo_core::{
    AmendTarget, Commit, DiscardAllTarget, DiscardTarget, FailureKind, OperationResult,
    RenameBranchTarget, Repository, RepositoryAction, RepositorySource, UndoCommitTarget,
};

use super::{git, git_stdout, test_repository};

#[test]
fn discards_selected_and_all_worktree_changes_without_touching_index_or_ignored_files() {
    let repo = test_repository();
    fs::write(repo.path().join("tracked.txt"), "staged\n").unwrap();
    git(repo.path(), &["add", "tracked.txt"]);
    fs::write(repo.path().join("tracked.txt"), "unstaged\n").unwrap();
    fs::write(repo.path().join("one.txt"), "one\n").unwrap();
    fs::write(repo.path().join("two.txt"), "two\n").unwrap();
    fs::write(repo.path().join(".gitignore"), "ignored.txt\n").unwrap();
    git(repo.path(), &["add", ".gitignore"]);
    git(repo.path(), &["commit", "-m", "ignore"]);
    fs::write(repo.path().join("ignored.txt"), "keep\n").unwrap();
    let source = crate::GitRepositorySource::new(repo.path());
    let snapshot = source.snapshot().unwrap();
    let tracked = snapshot
        .files
        .iter()
        .find(|file| file.path == Path::new("tracked.txt"))
        .unwrap()
        .clone();

    assert_eq!(
        source
            .apply(&RepositoryAction::Discard(Box::new(DiscardTarget {
                file: tracked,
            })))
            .unwrap(),
        OperationResult::Discard { paths: 1 }
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "staged\n"
    );

    let files = source
        .snapshot()
        .unwrap()
        .files
        .into_iter()
        .filter(|file| file.unstaged.is_some() || file.kind == diffo_core::ChangeKind::Untracked)
        .collect();
    source
        .apply(&RepositoryAction::DiscardAll(Box::new(DiscardAllTarget {
            files,
        })))
        .unwrap();

    assert!(!repo.path().join("one.txt").exists());
    assert!(!repo.path().join("two.txt").exists());
    assert!(repo.path().join("ignored.txt").exists());
    assert_eq!(git_stdout(repo.path(), &["show", ":tracked.txt"]), "staged");
}

#[test]
fn stash_apply_keeps_the_entry_and_drop_rechecks_its_identity() {
    let repo = test_repository();
    fs::write(repo.path().join("tracked.txt"), "staged\n").unwrap();
    git(repo.path(), &["add", "tracked.txt"]);
    fs::write(repo.path().join("untracked.txt"), "saved\n").unwrap();
    let source = crate::GitRepositorySource::new(repo.path());

    assert_eq!(
        source
            .apply(&RepositoryAction::Stash {
                message: "shelf".to_owned(),
            })
            .unwrap(),
        OperationResult::Stash {
            name: "stash@{0}".to_owned(),
        }
    );
    let stash = source.stashes().unwrap().remove(0);
    source
        .apply(&RepositoryAction::ApplyStash(Box::new(stash.clone())))
        .unwrap();
    assert_eq!(source.stashes().unwrap().len(), 1);
    assert_eq!(git_stdout(repo.path(), &["show", ":tracked.txt"]), "staged");
    assert!(repo.path().join("untracked.txt").exists());
    git(repo.path(), &["reset", "--hard"]);
    fs::remove_file(repo.path().join("untracked.txt")).unwrap();
    source
        .apply(&RepositoryAction::DropStash(Box::new(stash)))
        .unwrap();
    assert!(source.stashes().unwrap().is_empty());
}

#[test]
fn amend_and_undo_only_rewrite_the_captured_local_non_merge_head() {
    let repo = test_repository();
    fs::write(repo.path().join("local.txt"), "local\n").unwrap();
    git(repo.path(), &["add", "local.txt"]);
    git(repo.path(), &["commit", "-m", "local"]);
    let source = crate::GitRepositorySource::new(repo.path());
    let head = git_stdout(repo.path(), &["rev-parse", "HEAD"]);

    let result = source
        .apply(&RepositoryAction::Amend(Box::new(AmendTarget {
            expected_head: head,
            message: "amended".to_owned(),
        })))
        .unwrap();
    let amended = match result {
        OperationResult::Amend { hash } => hash,
        result => panic!("unexpected result: {result:?}"),
    };
    assert_eq!(
        git_stdout(repo.path(), &["log", "-1", "--format=%s"]),
        "amended"
    );

    let result = source
        .apply(&RepositoryAction::UndoLastCommit(Box::new(
            UndoCommitTarget {
                expected_head: amended,
            },
        )))
        .unwrap();
    assert!(matches!(result, OperationResult::UndoLastCommit { .. }));
    assert_eq!(
        git_stdout(repo.path(), &["diff", "--cached", "--name-only"]),
        "local.txt"
    );
}

#[test]
fn revert_creates_a_new_commit_and_aborts_conflicts() {
    let repo = test_repository();
    fs::write(repo.path().join("tracked.txt"), "change\n").unwrap();
    git(repo.path(), &["commit", "-am", "change"]);
    let source = crate::GitRepositorySource::new(repo.path());
    let commit = source.snapshot().unwrap().recent_commits[0].clone();

    assert!(matches!(
        source.apply(&RepositoryAction::Revert(Box::new(commit))),
        Ok(OperationResult::Revert { .. })
    ));
    assert_eq!(
        fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "base\n"
    );

    let stale = Commit {
        id: "not-a-commit".to_owned(),
        summary: "missing".to_owned(),
    };
    assert_eq!(
        source
            .apply(&RepositoryAction::Revert(Box::new(stale)))
            .unwrap_err()
            .kind,
        FailureKind::RefChanged
    );
}

#[test]
fn rename_clears_the_existing_upstream() {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "--bare", "remote.git"]);
    let repo = test_repository();
    git(
        repo.path(),
        &[
            "remote",
            "add",
            "origin",
            root.path().join("remote.git").to_str().unwrap(),
        ],
    );
    git(repo.path(), &["push", "-u", "origin", "main"]);
    let source = crate::GitRepositorySource::new(repo.path());
    let head = git_stdout(repo.path(), &["rev-parse", "HEAD"]);

    source
        .apply(&RepositoryAction::RenameBranch(Box::new(
            RenameBranchTarget {
                old_name: "main".to_owned(),
                old_full_ref: "refs/heads/main".to_owned(),
                object_id: head.clone(),
                new_name: "topic".to_owned(),
                had_upstream: true,
            },
        )))
        .unwrap();
    assert!(source.snapshot().unwrap().upstream.is_none());
}
