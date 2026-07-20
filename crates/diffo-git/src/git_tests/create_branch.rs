use std::{fs, path::Path};

use diffo_core::{
    BranchKind, CheckoutTarget, CreateBranchStartPoint, CreateBranchTarget, FailureKind, HeadState,
    OperationResult, Repository, RepositoryAction, RepositorySource,
};

use super::{git, test_repository};

#[test]
fn creates_and_checks_out_an_untracked_branch_without_changing_worktree_state() {
    let repo = test_repository();
    let source = crate::GitRepositorySource::new(repo.path());
    let expected_head = source.snapshot().unwrap().head;
    let expected_commit = match &expected_head {
        HeadState::Named { commit, .. } => commit.clone(),
        state => panic!("expected named head, got {state:?}"),
    };
    fs::write(repo.path().join("tracked.txt"), "unstaged\n").unwrap();
    fs::write(repo.path().join("staged.txt"), "staged\n").unwrap();
    git(repo.path(), &["add", "staged.txt"]);
    fs::write(repo.path().join("untracked.txt"), "untracked\n").unwrap();

    let result = source
        .apply(&RepositoryAction::CreateBranch(Box::new(
            CreateBranchTarget {
                name: "topic/nested".to_owned(),
                start_point: CreateBranchStartPoint::Head(expected_head),
            },
        )))
        .expect("create topic branch");

    assert_eq!(
        result,
        OperationResult::CreateBranch {
            branch: "topic/nested".to_owned()
        }
    );
    let snapshot = source.snapshot().unwrap();
    assert!(matches!(
        snapshot.head,
        HeadState::Named { name, commit }
            if name == "topic/nested" && commit == expected_commit
    ));
    assert!(snapshot.upstream.is_none());
    assert_eq!(
        fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "unstaged\n"
    );
    assert!(
        snapshot
            .files
            .iter()
            .any(|file| { file.path == Path::new("staged.txt") && file.staged.is_some() })
    );
    assert!(
        snapshot
            .files
            .iter()
            .any(|file| { file.path == Path::new("untracked.txt") && file.unstaged.is_some() })
    );
}

#[test]
fn creates_a_named_branch_from_detached_head() {
    let repo = test_repository();
    git(repo.path(), &["checkout", "--detach"]);
    let source = crate::GitRepositorySource::new(repo.path());
    let expected_head = source.snapshot().unwrap().head;

    let result = source
        .apply(&RepositoryAction::CreateBranch(Box::new(
            CreateBranchTarget {
                name: "detached-topic".to_owned(),
                start_point: CreateBranchStartPoint::Head(expected_head),
            },
        )))
        .unwrap();

    assert!(matches!(
        result,
        OperationResult::CreateBranch { branch } if branch == "detached-topic"
    ));
    assert!(matches!(
        source.snapshot().unwrap().head,
        HeadState::Named { name, .. } if name == "detached-topic"
    ));
}

#[test]
fn creates_from_a_selected_branch_and_rejects_a_moved_selection() {
    let repo = test_repository();
    git(repo.path(), &["branch", "base"]);
    let source = crate::GitRepositorySource::new(repo.path());
    let base = source
        .branches()
        .unwrap()
        .into_iter()
        .find(|branch| branch.kind == BranchKind::Local && branch.name == "base")
        .unwrap();
    let selected = CheckoutTarget {
        kind: base.kind,
        full_ref: base.full_ref,
        object_id: base.object_id,
    };
    fs::write(repo.path().join("later.txt"), "later\n").unwrap();
    git(repo.path(), &["add", "later.txt"]);
    git(repo.path(), &["commit", "-m", "advance main"]);

    source
        .apply(&RepositoryAction::CreateBranch(Box::new(
            CreateBranchTarget {
                name: "from-base".to_owned(),
                start_point: CreateBranchStartPoint::Branch(selected.clone()),
            },
        )))
        .expect("create from selected base");
    assert!(matches!(
        source.snapshot().unwrap().head,
        HeadState::Named { name, commit }
            if name == "from-base" && commit == selected.object_id
    ));

    git(repo.path(), &["checkout", "main"]);
    git(repo.path(), &["branch", "-f", "base", "main"]);
    let failure = source
        .apply(&RepositoryAction::CreateBranch(Box::new(
            CreateBranchTarget {
                name: "from-stale-base".to_owned(),
                start_point: CreateBranchStartPoint::Branch(selected),
            },
        )))
        .unwrap_err();
    assert_eq!(failure.kind, FailureKind::RefChanged);
    assert!(
        !source
            .branches()
            .unwrap()
            .iter()
            .any(|branch| branch.name == "from-stale-base")
    );
}

#[test]
fn rejects_duplicate_invalid_unborn_and_changed_heads_before_mutation() {
    let repo = test_repository();
    let source = crate::GitRepositorySource::new(repo.path());
    let expected_head = source.snapshot().unwrap().head;
    git(repo.path(), &["branch", "existing"]);
    let duplicate = source
        .apply(&RepositoryAction::CreateBranch(Box::new(
            CreateBranchTarget {
                name: "existing".to_owned(),
                start_point: CreateBranchStartPoint::Head(expected_head.clone()),
            },
        )))
        .unwrap_err();
    assert_eq!(duplicate.kind, FailureKind::BranchConflict);

    let invalid = source
        .apply(&RepositoryAction::CreateBranch(Box::new(
            CreateBranchTarget {
                name: "invalid?name".to_owned(),
                start_point: CreateBranchStartPoint::Head(expected_head.clone()),
            },
        )))
        .unwrap_err();
    assert_eq!(invalid.kind, FailureKind::BranchConflict);

    fs::write(repo.path().join("next.txt"), "next\n").unwrap();
    git(repo.path(), &["add", "next.txt"]);
    git(repo.path(), &["commit", "-m", "move HEAD"]);
    let changed = source
        .apply(&RepositoryAction::CreateBranch(Box::new(
            CreateBranchTarget {
                name: "stale-topic".to_owned(),
                start_point: CreateBranchStartPoint::Head(expected_head),
            },
        )))
        .unwrap_err();
    assert_eq!(changed.kind, FailureKind::RefChanged);
    assert!(
        !source
            .branches()
            .unwrap()
            .iter()
            .any(|branch| branch.name == "stale-topic")
    );

    let unborn = tempfile::tempdir().unwrap();
    git(unborn.path(), &["init", "--initial-branch=main"]);
    let unborn_source = crate::GitRepositorySource::new(unborn.path());
    let unborn_failure = unborn_source
        .apply(&RepositoryAction::CreateBranch(Box::new(
            CreateBranchTarget {
                name: "topic".to_owned(),
                start_point: CreateBranchStartPoint::Head(unborn_source.snapshot().unwrap().head),
            },
        )))
        .unwrap_err();
    assert_eq!(unborn_failure.kind, FailureKind::UnsupportedHead);
}
