use std::path::PathBuf;

use diffo_core::{
    ChangeKind, FailureKind, FileDiff, FileState, OperationFailure, OperationResult,
    RepositoryAction, RepositorySnapshot, SyncPlan, UpstreamState,
};

use super::{ChangeArea, DiffViewMode, Effect, FileKey, Message, Model, NetworkOperation, update};

fn model() -> Model {
    Model::new(RepositorySnapshot {
        files: vec![FileState {
            path: PathBuf::from("file.txt"),
            old_path: None,
            kind: ChangeKind::Modified,
            staged: Some(FileDiff {
                text: String::new(),
            }),
            unstaged: Some(FileDiff {
                text: String::new(),
            }),
        }],
        ..RepositorySnapshot::default()
    })
}

#[test]
fn updates_local_state() {
    let mut model = model();

    assert_eq!(update(&mut model, Message::ScrollDiffRight), None);
    assert_eq!(model.diff_horizontal_scroll, 4);
    assert_eq!(update(&mut model, Message::Quit), None);
    assert!(model.should_quit);
}

#[test]
fn commit_and_sync_are_independent_actions() {
    let mut model = model();
    assert!(model.commit_enabled());
    assert!(model.sync_enabled());
    assert_eq!(
        model.suggested_commit_message().as_deref(),
        Some("Update 1 file")
    );

    update(&mut model, Message::FocusCommitInput);
    for character in "ship it".chars() {
        update(&mut model, Message::CommitMessageInput(character));
    }
    assert_eq!(
        update(&mut model, Message::ExecuteCommit),
        Some(Effect::Repository(RepositoryAction::Commit(
            "ship it".to_owned()
        )))
    );
    assert_eq!(model.commit_message, "ship it");
    let refreshed = model.snapshot.clone();
    update(&mut model, Message::SnapshotLoaded(refreshed.clone()));
    assert_eq!(model.commit_message, "ship it");
    update(
        &mut model,
        Message::OperationCompleted(
            RepositoryAction::Commit("ship it".to_owned()),
            OperationResult::Commit {
                hash: "abc1234".to_owned(),
            },
            refreshed,
        ),
    );
    assert!(model.commit_message.is_empty());

    model.snapshot.files[0].staged = None;
    model.snapshot.upstream = Some(UpstreamState {
        name: "origin/main".to_owned(),
        ahead: 1,
        behind: 0,
    });
    assert!(!model.commit_enabled());
    assert!(model.sync_enabled());
    assert_eq!(
        update(&mut model, Message::ExecuteSync),
        Some(Effect::Repository(RepositoryAction::Sync))
    );
    assert!(!model.sync_enabled());
    assert_eq!(model.network_operation(), Some(NetworkOperation::Sync));
    let refreshed = model.snapshot.clone();
    update(&mut model, Message::SnapshotLoaded(refreshed.clone()));
    assert_eq!(model.network_operation(), Some(NetworkOperation::Sync));
    update(
        &mut model,
        Message::OperationCompleted(
            RepositoryAction::Sync,
            OperationResult::Sync {
                plan: Box::new(SyncPlan {
                    branch: "main".to_owned(),
                    upstream: "origin/main".to_owned(),
                    local_only: 1,
                    upstream_only: 0,
                }),
            },
            refreshed,
        ),
    );
    assert_eq!(model.network_operation(), None);
    assert!(model.sync_enabled());
    assert_eq!(
        update(&mut model, Message::ExecuteSync),
        Some(Effect::Repository(RepositoryAction::Sync))
    );
}

#[test]
fn generated_commit_message_is_used_when_input_is_empty() {
    let mut model = model();

    assert_eq!(
        update(&mut model, Message::ExecuteCommit),
        Some(Effect::Repository(RepositoryAction::Commit(
            "Update 1 file".to_owned()
        )))
    );
}

#[test]
fn passive_and_unrelated_results_cannot_finish_a_sync() {
    let mut model = model();
    model.snapshot.files[0].staged = None;
    model.snapshot.upstream = Some(UpstreamState {
        name: "origin/main".to_owned(),
        ahead: 1,
        behind: 0,
    });
    assert_eq!(
        update(&mut model, Message::ExecuteSync),
        Some(Effect::Repository(RepositoryAction::Sync))
    );

    let changed = model.snapshot.clone();
    update(&mut model, Message::SnapshotLoaded(changed.clone()));
    update(
        &mut model,
        Message::OperationFailed("watch failed".to_owned()),
    );
    update(
        &mut model,
        Message::OperationCompleted(
            RepositoryAction::Fetch,
            OperationResult::Fetch { updated_refs: 1 },
            changed,
        ),
    );
    update(
        &mut model,
        Message::ActionFailed(OperationFailure {
            action: RepositoryAction::Commit("unrelated".to_owned()),
            kind: FailureKind::Network,
            detail: "unrelated".to_owned(),
        }),
    );

    assert_eq!(model.network_operation(), Some(NetworkOperation::Sync));
    assert!(!model.sync_enabled());

    update(
        &mut model,
        Message::ActionFailed(OperationFailure {
            action: RepositoryAction::Sync,
            kind: FailureKind::Network,
            detail: "offline".to_owned(),
        }),
    );
    assert_eq!(model.network_operation(), None);
}

#[test]
fn scrolls_four_lines_in_the_arrow_direction() {
    let mut model = model();

    update(&mut model, Message::ScrollDiffDown);
    assert_eq!(model.diff_scroll, 4);
    update(&mut model, Message::ScrollDiffUp);
    assert_eq!(model.diff_scroll, 0);
    update(&mut model, Message::ScrollDiffUp);
    assert_eq!(model.diff_scroll, 0);
}

#[test]
fn scrolls_by_a_page() {
    let mut model = model();

    update(&mut model, Message::ScrollDiffPageDown(27));
    assert_eq!(model.diff_scroll, 27);
    update(&mut model, Message::ScrollDiffPageUp(27));
    assert_eq!(model.diff_scroll, 0);
}

#[test]
fn resizes_the_file_pane() {
    let mut model = model();

    update(&mut model, Message::BeginFilePaneResize);
    update(&mut model, Message::ResizeFilePane(64));
    update(&mut model, Message::EndFilePaneResize);
    assert_eq!(model.file_pane_percent, 64);
    assert!(!model.resizing_file_pane);

    update(&mut model, Message::BeginFilePaneResize);
    update(&mut model, Message::ResizeFilePane(100));
    assert_eq!(model.file_pane_percent, 80);
}

#[test]
fn toggles_diff_view_mode() {
    let mut model = model();
    assert_eq!(model.diff_view_mode, DiffViewMode::Inline);
    model.scroll_diff_down();
    model.scroll_diff_right();

    update(&mut model, Message::ToggleDiffView);
    assert_eq!(model.diff_view_mode, DiffViewMode::SideBySide);
    assert_eq!(model.diff_scroll, 4);
    assert_eq!(model.diff_horizontal_scroll, 4);

    update(&mut model, Message::ToggleDiffView);
    assert_eq!(model.diff_view_mode, DiffViewMode::Inline);
}

#[test]
fn returns_contextual_stage_effect() {
    let mut model = model();

    assert_eq!(
        update(&mut model, Message::ToggleStageSelected),
        Some(Effect::Repository(RepositoryAction::Stage(PathBuf::from(
            "file.txt"
        ))))
    );
    let mut refreshed = model.snapshot.clone();
    refreshed.files[0].unstaged = None;
    update(
        &mut model,
        Message::OperationCompleted(
            RepositoryAction::Stage(PathBuf::from("file.txt")),
            OperationResult::Stage,
            refreshed,
        ),
    );

    assert_eq!(
        update(&mut model, Message::ToggleStageSelected),
        Some(Effect::Repository(RepositoryAction::Unstage(
            PathBuf::from("file.txt")
        )))
    );
}

#[test]
fn returns_file_button_effects_without_changing_selection() {
    let mut model = model();
    assert_eq!(
        update(&mut model, Message::StageFile(PathBuf::from("file.txt"))),
        Some(Effect::Repository(RepositoryAction::Stage(PathBuf::from(
            "file.txt"
        ))))
    );
    assert_eq!(
        update(&mut model, Message::UnstageFile(PathBuf::from("file.txt"))),
        Some(Effect::Repository(RepositoryAction::Unstage(
            PathBuf::from("file.txt")
        )))
    );
}

#[test]
fn toggles_all_changes_contextually() {
    let mut model = model();
    assert_eq!(
        update(&mut model, Message::ToggleStageAll),
        Some(Effect::Repository(RepositoryAction::StageAll))
    );

    model.snapshot.files[0].unstaged = None;
    assert_eq!(
        update(&mut model, Message::ToggleStageAll),
        Some(Effect::Repository(RepositoryAction::UnstageAll))
    );

    assert_eq!(
        update(&mut model, Message::UnstageAll),
        Some(Effect::Repository(RepositoryAction::UnstageAll))
    );
}

#[test]
fn selects_semantic_file_key() {
    let mut model = model();
    let staged = FileKey {
        path: PathBuf::from("file.txt"),
        area: ChangeArea::Staged,
    };

    update(&mut model, Message::SelectFile(staged.clone()));

    assert_eq!(model.selected, Some(staged));
}

#[test]
fn handles_runtime_results_as_messages() {
    let mut model = model();
    let snapshot = RepositorySnapshot::default();

    update(&mut model, Message::SnapshotLoaded(snapshot.clone()));
    assert_eq!(model.snapshot, snapshot);
    update(
        &mut model,
        Message::OperationFailed("action failed".to_owned()),
    );
    assert_eq!(model.error.as_deref(), Some("action failed"));
}
