use std::{fs, process::Command};

use diffo_core::{
    FailureKind, HeadState, MergeRefKind, MergeTarget, OperationResult, Repository,
    RepositoryAction, RepositoryOperationState, RepositorySource,
};

use super::{git, test_repository};

fn target(source: &crate::GitRepositorySource, name: &str) -> MergeTarget {
    let selected = source
        .merge_refs()
        .unwrap()
        .into_iter()
        .find(|item| item.name == name)
        .unwrap();
    MergeTarget {
        kind: selected.kind,
        name: selected.name,
        full_ref: selected.full_ref,
        object_id: selected.object_id,
        commit_id: selected.commit_id,
        expected_head: source.snapshot().unwrap().head,
    }
}

fn git_stdout(repo: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

#[test]
fn discovers_local_remote_and_peeled_tag_refs() {
    let repo = test_repository();
    git(repo.path(), &["branch", "topic"]);
    git(repo.path(), &["remote", "add", "origin", "."]);
    git(
        repo.path(),
        &["update-ref", "refs/remotes/origin/topic", "HEAD"],
    );
    git(repo.path(), &["tag", "-a", "v1", "-m", "version one"]);
    let source = crate::GitRepositorySource::new(repo.path());

    let refs = source.merge_refs().unwrap();

    assert!(refs.iter().any(|item| {
        item.kind == MergeRefKind::Local && item.name == "topic" && item.object_id == item.commit_id
    }));
    assert!(
        refs.iter()
            .any(|item| { item.kind == MergeRefKind::Remote && item.name == "origin/topic" })
    );
    assert!(refs.iter().any(|item| {
        item.kind == MergeRefKind::Tag && item.name == "v1" && item.object_id != item.commit_id
    }));
}

#[test]
fn fast_forwards_and_creates_a_merge_commit_when_needed() {
    let repo = test_repository();
    git(repo.path(), &["switch", "-c", "topic"]);
    fs::write(repo.path().join("topic.txt"), "topic\n").unwrap();
    git(repo.path(), &["add", "topic.txt"]);
    git(repo.path(), &["commit", "-m", "topic"]);
    let topic = git_stdout(repo.path(), &["rev-parse", "HEAD"]);
    git(repo.path(), &["switch", "main"]);
    let source = crate::GitRepositorySource::new(repo.path());

    let fast_forward = source
        .apply(&RepositoryAction::Merge(Box::new(target(&source, "topic"))))
        .unwrap();

    assert_eq!(
        fast_forward,
        OperationResult::Merge {
            name: "topic".to_owned(),
            conflicts: 0,
        }
    );
    assert_eq!(git_stdout(repo.path(), &["rev-parse", "HEAD"]), topic);

    git(repo.path(), &["switch", "-c", "second-topic", "HEAD~1"]);
    fs::write(repo.path().join("second.txt"), "second\n").unwrap();
    git(repo.path(), &["add", "second.txt"]);
    git(repo.path(), &["commit", "-m", "second topic"]);
    git(repo.path(), &["switch", "main"]);

    source
        .apply(&RepositoryAction::Merge(Box::new(target(
            &source,
            "second-topic",
        ))))
        .unwrap();

    assert_eq!(
        git_stdout(repo.path(), &["show", "-s", "--format=%P", "HEAD"])
            .split_whitespace()
            .count(),
        2
    );
}

#[test]
fn merges_an_annotated_tag() {
    let repo = test_repository();
    git(repo.path(), &["switch", "-c", "topic"]);
    fs::write(repo.path().join("tagged.txt"), "tagged\n").unwrap();
    git(repo.path(), &["add", "tagged.txt"]);
    git(repo.path(), &["commit", "-m", "tagged"]);
    git(repo.path(), &["tag", "-a", "v1", "-m", "version one"]);
    git(repo.path(), &["switch", "main"]);
    let source = crate::GitRepositorySource::new(repo.path());

    let result = source
        .apply(&RepositoryAction::Merge(Box::new(target(&source, "v1"))))
        .unwrap();

    assert_eq!(
        result,
        OperationResult::Merge {
            name: "v1".to_owned(),
            conflicts: 0,
        }
    );
    git(repo.path(), &["merge-base", "--is-ancestor", "v1", "HEAD"]);
}

#[test]
fn keeps_conflicts_visible_and_abort_restores_the_destination() {
    let repo = test_repository();
    git(repo.path(), &["switch", "-c", "topic"]);
    fs::write(repo.path().join("tracked.txt"), "topic\n").unwrap();
    git(repo.path(), &["commit", "-am", "topic"]);
    git(repo.path(), &["switch", "main"]);
    fs::write(repo.path().join("tracked.txt"), "main\n").unwrap();
    git(repo.path(), &["commit", "-am", "main"]);
    let before = git_stdout(repo.path(), &["rev-parse", "HEAD"]);
    let source = crate::GitRepositorySource::new(repo.path());

    let result = source
        .apply(&RepositoryAction::Merge(Box::new(target(&source, "topic"))))
        .unwrap();

    assert_eq!(
        result,
        OperationResult::Merge {
            name: "topic".to_owned(),
            conflicts: 1,
        }
    );
    let conflicted = source.snapshot().unwrap();
    assert_eq!(conflicted.operation, RepositoryOperationState::Merge);
    assert!(
        conflicted
            .files
            .iter()
            .any(|file| file.kind == diffo_core::ChangeKind::Conflicted)
    );

    assert_eq!(
        source.apply(&RepositoryAction::AbortMerge).unwrap(),
        OperationResult::AbortMerge
    );
    let restored = source.snapshot().unwrap();
    assert_eq!(restored.operation, RepositoryOperationState::None);
    assert_eq!(git_stdout(repo.path(), &["rev-parse", "HEAD"]), before);
    assert_eq!(
        fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "main\n"
    );

    let external_merge = Command::new("git")
        .args(["merge", "--no-edit", "topic"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(!external_merge.status.success());
    assert_eq!(
        source.snapshot().unwrap().operation,
        RepositoryOperationState::Merge
    );
    source.apply(&RepositoryAction::AbortMerge).unwrap();
}

#[test]
fn rejects_moved_sources_changed_heads_and_unborn_destinations() {
    let repo = test_repository();
    git(repo.path(), &["branch", "topic"]);
    let source = crate::GitRepositorySource::new(repo.path());
    let moved = target(&source, "topic");
    fs::write(repo.path().join("later.txt"), "later\n").unwrap();
    git(repo.path(), &["add", "later.txt"]);
    git(repo.path(), &["commit", "-m", "later"]);
    git(repo.path(), &["branch", "-f", "topic", "HEAD"]);
    assert_eq!(
        source
            .apply(&RepositoryAction::Merge(Box::new(moved)))
            .unwrap_err()
            .kind,
        FailureKind::RefChanged
    );

    git(repo.path(), &["branch", "stable", "HEAD~1"]);
    let changed_head = target(&source, "stable");
    fs::write(repo.path().join("newer.txt"), "newer\n").unwrap();
    git(repo.path(), &["add", "newer.txt"]);
    git(repo.path(), &["commit", "-m", "newer"]);
    assert_eq!(
        source
            .apply(&RepositoryAction::Merge(Box::new(changed_head)))
            .unwrap_err()
            .kind,
        FailureKind::RefChanged
    );

    let unborn = tempfile::tempdir().unwrap();
    git(unborn.path(), &["init", "--initial-branch=main"]);
    let unborn_source = crate::GitRepositorySource::new(unborn.path());
    let failure = unborn_source
        .apply(&RepositoryAction::Merge(Box::new(MergeTarget {
            kind: MergeRefKind::Local,
            name: "topic".to_owned(),
            full_ref: "refs/heads/topic".to_owned(),
            object_id: "missing".to_owned(),
            commit_id: "missing".to_owned(),
            expected_head: HeadState::Unborn {
                name: "main".to_owned(),
            },
        })))
        .unwrap_err();
    assert!(matches!(
        failure.kind,
        FailureKind::RefChanged | FailureKind::UnsupportedHead
    ));
}

#[test]
fn merge_is_allowed_from_detached_head() {
    let repo = test_repository();
    git(repo.path(), &["switch", "-c", "topic"]);
    fs::write(repo.path().join("topic.txt"), "topic\n").unwrap();
    git(repo.path(), &["add", "topic.txt"]);
    git(repo.path(), &["commit", "-m", "topic"]);
    git(repo.path(), &["checkout", "--detach", "HEAD~1"]);
    let source = crate::GitRepositorySource::new(repo.path());
    assert!(matches!(
        source.snapshot().unwrap().head,
        HeadState::Detached { .. }
    ));

    source
        .apply(&RepositoryAction::Merge(Box::new(target(&source, "topic"))))
        .unwrap();

    assert!(matches!(
        source.snapshot().unwrap().head,
        HeadState::Detached { .. }
    ));
}
