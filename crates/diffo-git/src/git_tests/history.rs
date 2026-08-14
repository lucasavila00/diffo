use std::fs;

use diffo_core::Repository;

use super::{git, test_repository};

#[test]
fn lists_the_complete_checkout_history_newest_first() {
    let repo = test_repository();
    fs::write(repo.path().join("second.txt"), "second\n").unwrap();
    git(repo.path(), &["add", "second.txt"]);
    git(repo.path(), &["commit", "-m", "second commit"]);
    let source = crate::GitRepositorySource::new(repo.path());

    let history = source.checkout_history().unwrap();

    assert_eq!(history.commits.len(), 2);
    assert_eq!(history.commits[0].summary, "second commit");
    assert_eq!(history.commits[1].summary, "Base commit");
    assert_eq!(
        history.head_commit.as_deref(),
        Some(history.commits[0].id.as_str())
    );
}

#[test]
fn root_commit_patch_is_compared_with_the_empty_tree() {
    let repo = test_repository();
    let source = crate::GitRepositorySource::new(repo.path());
    let root = source.checkout_history().unwrap().commits.pop().unwrap();

    let patch = source.commit_patch(&root.id).unwrap();

    assert!(patch.contains("new file mode"), "{patch}");
    assert!(patch.contains("+base"), "{patch}");
}

#[test]
fn merge_commit_patch_uses_the_first_parent() {
    let repo = test_repository();
    git(repo.path(), &["switch", "-c", "topic"]);
    fs::write(repo.path().join("topic.txt"), "topic\n").unwrap();
    git(repo.path(), &["add", "topic.txt"]);
    git(repo.path(), &["commit", "-m", "topic"]);
    git(repo.path(), &["switch", "main"]);
    fs::write(repo.path().join("main.txt"), "main\n").unwrap();
    git(repo.path(), &["add", "main.txt"]);
    git(repo.path(), &["commit", "-m", "main"]);
    git(
        repo.path(),
        &["merge", "--no-ff", "topic", "-m", "merge topic"],
    );
    let source = crate::GitRepositorySource::new(repo.path());
    let merge = source.checkout_history().unwrap().commits[0].clone();

    let patch = source.commit_patch(&merge.id).unwrap();

    assert!(patch.contains("topic.txt"), "{patch}");
    assert!(!patch.contains("main.txt"), "{patch}");
    assert!(!patch.contains("diff --cc"), "{patch}");
}

#[test]
fn unborn_checkout_has_empty_history() {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "--initial-branch=main"]);
    let source = crate::GitRepositorySource::new(repo.path());

    let history = source.checkout_history().unwrap();

    assert_eq!(history.head_commit, None);
    assert!(history.commits.is_empty());
}
