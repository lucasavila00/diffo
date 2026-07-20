use super::*;

#[test]
fn discovers_and_checks_out_a_local_branch_by_typed_ref() {
    let repo = test_repository();
    git(repo.path(), &["branch", "topic"]);
    let source = super::super::GitRepositorySource::new(repo.path());
    let branches = source.branches().expect("branches");
    let topic = branches
        .iter()
        .find(|branch| branch.kind == BranchKind::Local && branch.name == "topic")
        .expect("topic branch");
    assert!(topic.tip_commit_unix_seconds.is_some());

    let result = source
        .apply(&RepositoryAction::Checkout(Box::new(CheckoutTarget {
            kind: topic.kind,
            full_ref: topic.full_ref.clone(),
            object_id: topic.object_id.clone(),
        })))
        .expect("checkout topic");

    assert_eq!(
        result,
        OperationResult::Checkout {
            branch: "topic".to_owned()
        }
    );
    assert!(matches!(
        source.snapshot().unwrap().head,
        HeadState::Named { name, .. } if name == "topic"
    ));
}
