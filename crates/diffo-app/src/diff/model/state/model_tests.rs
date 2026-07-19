use std::path::{Path, PathBuf};

use diffo_core::{
    ChangeKind, FailureKind, FileDiff, FileState, OperationFailure, OperationResult,
    RepositoryAction, RepositorySnapshot,
};

use super::{ChangeArea, FileKey, Model, ToastKind, ToastQueue};

fn snapshot() -> RepositorySnapshot {
    RepositorySnapshot {
        files: vec![
            FileState {
                path: PathBuf::from("both.txt"),
                old_path: None,
                kind: ChangeKind::Modified,
                staged: Some(FileDiff {
                    text: String::new(),
                }),
                unstaged: Some(FileDiff {
                    text: String::new(),
                }),
            },
            FileState {
                path: PathBuf::from("new.txt"),
                old_path: None,
                kind: ChangeKind::Untracked,
                staged: None,
                unstaged: None,
            },
        ],
        ..RepositorySnapshot::default()
    }
}

#[test]
fn navigates_both_groups() {
    let mut app = Model::new(snapshot());
    assert_eq!(
        app.selected.as_ref().expect("selection").path,
        PathBuf::from("both.txt")
    );

    assert_eq!(
        app.selected.as_ref().expect("selection").area,
        ChangeArea::Unstaged
    );
    app.select_previous();
    assert_eq!(
        app.selected.as_ref().expect("selection").path,
        PathBuf::from("both.txt")
    );
    assert_eq!(
        app.selected.as_ref().expect("selection").area,
        ChangeArea::Staged
    );
    app.select_next();
    assert_eq!(
        app.selected.as_ref().expect("selection").area,
        ChangeArea::Unstaged
    );
    app.select_next();
    assert_eq!(
        app.selected.as_ref().expect("selection").path,
        PathBuf::from("new.txt")
    );
}

#[test]
fn creates_actions_for_the_selected_group() {
    let mut app = Model::new(snapshot());
    assert_eq!(app.unstage_selected(), None);
    assert_eq!(
        app.stage_selected(),
        Some(RepositoryAction::Stage(PathBuf::from("both.txt")))
    );

    app.select_previous();
    assert_eq!(app.stage_selected(), None);
    assert_eq!(
        app.unstage_selected(),
        Some(RepositoryAction::Unstage(PathBuf::from("both.txt")))
    );
}

#[test]
fn stage_file_action_continues_until_no_unstaged_files_remain() {
    let mut app = Model::new(snapshot());
    assert_eq!(
        app.selected,
        Some(FileKey {
            path: PathBuf::from("both.txt"),
            area: ChangeArea::Unstaged,
        })
    );

    assert_eq!(
        app.toggle_stage_selected(),
        Some(RepositoryAction::Stage(PathBuf::from("both.txt")))
    );
    assert_eq!(app.toggle_stage_selected(), None, "stage action is pending");
    let mut refreshed = snapshot();
    refreshed.files[0].unstaged = None;
    app.complete_operation(
        &RepositoryAction::Stage(PathBuf::from("both.txt")),
        &OperationResult::Stage,
        refreshed,
    );

    assert_eq!(
        app.selected,
        Some(FileKey {
            path: PathBuf::from("new.txt"),
            area: ChangeArea::Unstaged,
        })
    );

    assert_eq!(
        app.toggle_stage_selected(),
        Some(RepositoryAction::Stage(PathBuf::from("new.txt")))
    );
    let mut finished = app.snapshot.clone();
    let new_file = finished
        .files
        .iter_mut()
        .find(|file| file.path == Path::new("new.txt"))
        .expect("new file");
    new_file.kind = ChangeKind::Added;
    new_file.staged = Some(FileDiff {
        text: String::new(),
    });
    app.complete_operation(
        &RepositoryAction::Stage(PathBuf::from("new.txt")),
        &OperationResult::Stage,
        finished,
    );

    assert!(
        app.snapshot
            .files
            .iter()
            .all(|file| file.unstaged.is_none() && file.kind != ChangeKind::Untracked)
    );
    assert_eq!(
        app.selected.as_ref().map(|selected| selected.area),
        Some(ChangeArea::Staged)
    );
}

#[test]
fn failed_stage_file_action_keeps_the_reviewed_file_open() {
    let mut app = Model::new(snapshot());
    let selected = app.selected.clone();
    let action = app.toggle_stage_selected().expect("stage action");

    app.fail_operation(&OperationFailure {
        action: action.clone(),
        kind: FailureKind::Unknown,
        detail: "stage failed".to_owned(),
    });

    assert_eq!(app.selected, selected);
    assert_eq!(app.toggle_stage_selected(), Some(action));
}

#[test]
fn stage_file_action_falls_back_to_an_available_unstaged_file() {
    let mut initial = snapshot();
    initial.files.push(FileState {
        path: PathBuf::from("later.txt"),
        old_path: None,
        kind: ChangeKind::Untracked,
        staged: None,
        unstaged: None,
    });
    let mut app = Model::new(initial.clone());
    assert_eq!(
        app.toggle_stage_selected(),
        Some(RepositoryAction::Stage(PathBuf::from("both.txt")))
    );

    initial.files[0].unstaged = None;
    initial
        .files
        .retain(|file| file.path != Path::new("new.txt"));
    app.complete_operation(
        &RepositoryAction::Stage(PathBuf::from("both.txt")),
        &OperationResult::Stage,
        initial,
    );

    assert_eq!(
        app.selected,
        Some(FileKey {
            path: PathBuf::from("later.txt"),
            area: ChangeArea::Unstaged,
        })
    );
}

#[test]
fn keeps_selection_after_refresh() {
    let mut app = Model::new(snapshot());
    app.select_previous();
    let selected = FileKey {
        path: PathBuf::from("both.txt"),
        area: ChangeArea::Staged,
    };

    app.repository_changed(snapshot());

    assert_eq!(app.selected, Some(selected));
}

#[test]
fn preserves_scroll_when_the_selected_file_changes_content() {
    let mut app = Model::new(snapshot());
    app.diff_scroll = 12;
    app.diff_horizontal_scroll = 8;

    app.repository_changed(snapshot());
    assert_eq!(app.diff_scroll, 12);
    assert_eq!(app.diff_horizontal_scroll, 8);

    let mut changed = snapshot();
    changed.files[0]
        .staged
        .as_mut()
        .expect("staged diff")
        .text
        .push_str("changed");
    app.repository_changed(changed);
    assert_eq!(app.diff_scroll, 12);
    assert_eq!(app.diff_horizontal_scroll, 8);
}

#[test]
fn preserves_commit_input_focus_across_repository_refresh() {
    let mut app = Model::new(snapshot());
    app.focus_commit_input();

    app.repository_changed(snapshot());
    app.commit_message_input('x');

    assert!(app.commit_input_focused());
    assert_eq!(app.commit_message, "x");
}

#[test]
fn edits_commit_message_at_a_preserved_character_cursor() {
    let mut app = Model::new(snapshot());
    app.focus_commit_input();
    for character in "ac".chars() {
        app.commit_message_input(character);
    }
    app.commit_message_cursor_left();
    app.commit_message_input('b');
    app.blur_commit_input();
    app.focus_commit_input();

    assert_eq!(app.commit_message, "abc");
    assert_eq!(app.commit_message_cursor(), 2);
    app.commit_message_backspace();
    assert_eq!(app.commit_message, "ac");
}

#[test]
fn queues_replaces_limits_and_dismisses_toasts() {
    let mut toasts = ToastQueue::new();
    for updated_refs in 1..=4 {
        toasts.show(ToastKind::Success, format!("Fetched {updated_refs} refs"));
    }
    assert_eq!(toasts.as_slice().len(), 3);
    assert_eq!(toasts.as_slice()[0].title, "Fetched 4 refs");

    toasts.show(ToastKind::Success, "Fetched 4 refs");
    assert_eq!(toasts.as_slice().len(), 3);
    assert_eq!(
        toasts
            .as_slice()
            .iter()
            .filter(|toast| toast.title == "Fetched 4 refs")
            .count(),
        1
    );
    let id = toasts.as_slice()[0].id;
    toasts.dismiss(id);
    assert!(toasts.as_slice().iter().all(|toast| toast.id != id));
}
