use std::fs;

use diffo_core::{DeleteBranchTarget, FailureKind, OperationResult, Repository, RepositoryAction};

use super::{git, test_repository};

fn target(source: &crate::GitRepositorySource, name: &str, force: bool) -> DeleteBranchTarget {
    let branch = source
        .branches()
        .unwrap()
        .into_iter()
        .find(|branch| branch.name == name)
        .unwrap();
    DeleteBranchTarget {
        name: branch.name,
        full_ref: branch.full_ref,
        object_id: branch.object_id,
        force,
    }
}

#[test]
fn deletes_a_merged_branch_without_force() {
    let repo = test_repository();
    git(repo.path(), &["branch", "merged"]);
    let source = crate::GitRepositorySource::new(repo.path());

    let result = source
        .apply(&RepositoryAction::DeleteBranch(Box::new(target(
            &source, "merged", false,
        ))))
        .unwrap();

    assert_eq!(
        result,
        OperationResult::DeleteBranch {
            branch: "merged".to_owned(),
        }
    );
    assert!(
        source
            .branches()
            .unwrap()
            .iter()
            .all(|branch| branch.name != "merged")
    );
}

#[test]
fn deletes_a_branch_merged_into_its_configured_upstream() {
    let repo = test_repository();
    git(repo.path(), &["switch", "-c", "topic"]);
    fs::write(repo.path().join("topic.txt"), "topic\n").unwrap();
    git(repo.path(), &["add", "topic.txt"]);
    git(repo.path(), &["commit", "-m", "topic"]);
    git(repo.path(), &["branch", "upstream"]);
    git(
        repo.path(),
        &["branch", "--set-upstream-to=upstream", "topic"],
    );
    git(repo.path(), &["switch", "main"]);
    fs::write(repo.path().join("main.txt"), "main\n").unwrap();
    git(repo.path(), &["add", "main.txt"]);
    git(repo.path(), &["commit", "-m", "main"]);
    let source = crate::GitRepositorySource::new(repo.path());

    let result = source
        .apply(&RepositoryAction::DeleteBranch(Box::new(target(
            &source, "topic", false,
        ))))
        .unwrap();

    assert_eq!(
        result,
        OperationResult::DeleteBranch {
            branch: "topic".to_owned(),
        }
    );
}

#[test]
fn rejects_a_selected_branch_that_was_removed() {
    let repo = test_repository();
    git(repo.path(), &["branch", "removed"]);
    let source = crate::GitRepositorySource::new(repo.path());
    let removed = target(&source, "removed", false);
    git(repo.path(), &["branch", "-D", "removed"]);

    let failure = source
        .apply(&RepositoryAction::DeleteBranch(Box::new(removed)))
        .unwrap_err();

    assert_eq!(failure.kind, FailureKind::RefChanged);
}

#[test]
fn unmerged_branch_requires_an_explicit_forced_retry() {
    let repo = test_repository();
    git(repo.path(), &["switch", "-c", "topic"]);
    fs::write(repo.path().join("topic.txt"), "topic\n").unwrap();
    git(repo.path(), &["add", "topic.txt"]);
    git(repo.path(), &["commit", "-m", "topic"]);
    git(repo.path(), &["switch", "main"]);
    let source = crate::GitRepositorySource::new(repo.path());
    let selected = target(&source, "topic", false);

    let failure = source
        .apply(&RepositoryAction::DeleteBranch(Box::new(selected.clone())))
        .unwrap_err();
    assert_eq!(failure.kind, FailureKind::BranchNotFullyMerged);
    assert!(
        source
            .branches()
            .unwrap()
            .iter()
            .any(|branch| branch.name == "topic")
    );

    let result = source
        .apply(&RepositoryAction::DeleteBranch(Box::new(
            DeleteBranchTarget {
                force: true,
                ..selected
            },
        )))
        .unwrap();
    assert_eq!(
        result,
        OperationResult::DeleteBranch {
            branch: "topic".to_owned(),
        }
    );
}

#[test]
fn rejects_a_moved_or_current_selected_branch_before_mutation() {
    let repo = test_repository();
    git(repo.path(), &["branch", "topic"]);
    let source = crate::GitRepositorySource::new(repo.path());
    let stale = target(&source, "topic", true);
    fs::write(repo.path().join("later.txt"), "later\n").unwrap();
    git(repo.path(), &["add", "later.txt"]);
    git(repo.path(), &["commit", "-m", "later"]);
    git(repo.path(), &["branch", "-f", "topic", "HEAD"]);

    let moved = source
        .apply(&RepositoryAction::DeleteBranch(Box::new(stale)))
        .unwrap_err();
    assert_eq!(moved.kind, FailureKind::RefChanged);
    assert!(
        source
            .branches()
            .unwrap()
            .iter()
            .any(|branch| branch.name == "topic")
    );

    git(repo.path(), &["switch", "topic"]);
    let current = source
        .apply(&RepositoryAction::DeleteBranch(Box::new(target(
            &source, "topic", true,
        ))))
        .unwrap_err();
    assert_eq!(current.kind, FailureKind::RefChanged);
    assert!(
        source
            .branches()
            .unwrap()
            .iter()
            .any(|branch| branch.name == "topic")
    );
}

#[test]
fn git_rejects_a_branch_checked_out_in_another_worktree() {
    let repo = test_repository();
    git(repo.path(), &["branch", "topic"]);
    let linked = repo.path().join("linked");
    git(
        repo.path(),
        &["worktree", "add", linked.to_str().unwrap(), "topic"],
    );
    let source = crate::GitRepositorySource::new(repo.path());

    let failure = source
        .apply(&RepositoryAction::DeleteBranch(Box::new(target(
            &source, "topic", true,
        ))))
        .unwrap_err();

    assert_eq!(failure.kind, FailureKind::Unknown);
    assert!(
        source
            .branches()
            .unwrap()
            .iter()
            .any(|branch| branch.name == "topic")
    );
}
