use std::path::{Path, PathBuf};

use diffo_core::{
    Repository, RepositoryAction, RepositorySource,
    fixture_source::{FixtureRepositorySource, MutableFixtureRepository},
};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("repository-state.ron")
}

#[test]
fn repository_snapshot() {
    let snapshot = FixtureRepositorySource::new(fixture())
        .snapshot()
        .expect("fixture should load");

    insta::assert_ron_snapshot!(snapshot);
}

#[test]
fn repository_snapshot_after_mock_stage() {
    let repository = MutableFixtureRepository::new(fixture()).expect("fixture should load");
    repository
        .apply(&RepositoryAction::Stage(PathBuf::from("README.md")))
        .expect("mock stage should work");

    insta::assert_ron_snapshot!(repository.snapshot().expect("snapshot after mock stage"));
}
