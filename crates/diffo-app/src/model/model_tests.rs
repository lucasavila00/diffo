use std::path::PathBuf;

use diffo_core::{
    AccessMode, ChangeKind, FileDiff, FileState, OperationResult, RepositoryAction,
    RepositorySnapshot,
};

use super::{ChangeArea, FileKey, Model};

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
    let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
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
        app.selected.as_ref().expect("selection").path,
        PathBuf::from("both.txt")
    );
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
    let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
    assert_eq!(app.stage_selected(), None);
    assert_eq!(
        app.unstage_selected(),
        Some(RepositoryAction::Unstage(PathBuf::from("both.txt")))
    );

    app.select_next();
    assert_eq!(app.unstage_selected(), None);
    assert_eq!(
        app.stage_selected(),
        Some(RepositoryAction::Stage(PathBuf::from("both.txt")))
    );
}

#[test]
fn staging_for_review_selects_the_next_unstaged_file_after_refresh() {
    let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
    app.select_next();
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
    let mut refreshed = snapshot();
    refreshed.files[0].unstaged = None;
    app.complete_operation(&OperationResult::Stage, refreshed);

    assert_eq!(
        app.selected,
        Some(FileKey {
            path: PathBuf::from("new.txt"),
            area: ChangeArea::Unstaged,
        })
    );
}

#[test]
fn keeps_selection_after_refresh() {
    let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
    let selected = FileKey {
        path: PathBuf::from("both.txt"),
        area: ChangeArea::Staged,
    };

    app.repository_changed(snapshot());

    assert_eq!(app.selected, Some(selected));
}

#[test]
fn preserves_scroll_when_the_selected_file_changes_content() {
    let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
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
    let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
    app.focus_commit_input();

    app.repository_changed(snapshot());
    app.commit_message_input('x');

    assert!(app.commit_input_focused());
    assert_eq!(app.commit_message, "x");
}

#[test]
fn edits_commit_message_at_a_preserved_character_cursor() {
    let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
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
fn read_only_mode_blocks_actions() {
    let mut app = Model::new(snapshot(), AccessMode::ReadOnly);

    assert_eq!(app.stage_selected(), None);
    assert_eq!(app.toggle_stage_all(), None);
    app.focus_commit_input();
    assert!(!app.commit_input_focused());
}

#[test]
fn queues_replaces_limits_and_dismisses_toasts() {
    let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
    for updated_refs in 1..=4 {
        app.open_command_palette();
        assert_eq!(
            app.execute_selected_command(),
            Some(RepositoryAction::Fetch)
        );
        app.complete_operation(&OperationResult::Fetch { updated_refs }, snapshot());
    }
    assert_eq!(app.toasts.len(), 3);
    assert_eq!(app.toasts[0].title, "Fetched 4 refs");

    app.open_command_palette();
    assert_eq!(
        app.execute_selected_command(),
        Some(RepositoryAction::Fetch)
    );
    app.complete_operation(&OperationResult::Fetch { updated_refs: 4 }, snapshot());
    assert_eq!(app.toasts.len(), 3);
    assert_eq!(
        app.toasts
            .iter()
            .filter(|toast| toast.title == "Fetched 4 refs")
            .count(),
        1
    );
    let id = app.toasts[0].id;
    app.dismiss_toast(id);
    assert!(app.toasts.iter().all(|toast| toast.id != id));
}
