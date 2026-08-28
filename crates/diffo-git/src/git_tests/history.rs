use std::{fs, path::Path};

use diffo_core::{ChangeKind, Repository};

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
fn commit_review_lists_files_and_file_patch_has_full_context() {
    let repo = test_repository();
    let original = (0..20)
        .map(|index| format!("context-{index:02}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(repo.path().join("long.txt"), &original).unwrap();
    git(repo.path(), &["add", "long.txt"]);
    git(repo.path(), &["commit", "-m", "add long file"]);
    let changed = original.replace("context-10", "changed-10");
    fs::write(repo.path().join("long.txt"), changed).unwrap();
    git(repo.path(), &["add", "long.txt"]);
    git(repo.path(), &["commit", "-m", "change long file"]);
    let source = crate::GitRepositorySource::new(repo.path());
    let commit = source.checkout_history().unwrap().commits[0].clone();

    let review = source.commit_review(&commit.id).unwrap();
    let file_patch = source
        .commit_file_patch(&commit.id, Path::new("long.txt"), None)
        .unwrap();

    assert_eq!(review.files.len(), 1);
    assert_eq!(review.files[0].path, Path::new("long.txt"));
    assert_eq!(review.files[0].kind, ChangeKind::Modified);
    assert!(!review.patch.contains("context-00"), "{}", review.patch);
    assert!(!review.patch.contains("context-19"), "{}", review.patch);
    assert!(file_patch.contains("context-00"), "{file_patch}");
    assert!(file_patch.contains("context-19"), "{file_patch}");
    assert!(file_patch.contains("-context-10"), "{file_patch}");
    assert!(file_patch.contains("+changed-10"), "{file_patch}");
}

#[test]
fn commit_review_file_order_matches_aggregate_patch_sections_with_renames() {
    let repo = test_repository();
    fs::write(repo.path().join("zeta.txt"), "zeta\n").unwrap();
    fs::write(repo.path().join("alpha.txt"), "alpha\n").unwrap();
    git(repo.path(), &["mv", "tracked.txt", "middle.txt"]);
    git(repo.path(), &["add", "alpha.txt", "zeta.txt"]);
    git(repo.path(), &["commit", "-m", "ordered files"]);
    let source = crate::GitRepositorySource::new(repo.path());
    let commit = source.checkout_history().unwrap().commits[0].clone();

    let review = source.commit_review(&commit.id).unwrap();
    let headers = review
        .patch
        .lines()
        .filter(|line| line.starts_with("diff --git "))
        .collect::<Vec<_>>();

    assert_eq!(headers.len(), review.files.len());
    for (header, file) in headers.iter().zip(&review.files) {
        assert!(
            header.ends_with(&format!(" b/{}", file.path.display())),
            "{header:?} did not match {:?}",
            file.path
        );
    }
    let renamed = review
        .files
        .iter()
        .find(|file| file.path == Path::new("middle.txt"))
        .unwrap();
    assert_eq!(renamed.old_path.as_deref(), Some(Path::new("tracked.txt")));
    assert_eq!(renamed.kind, ChangeKind::Renamed);
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
