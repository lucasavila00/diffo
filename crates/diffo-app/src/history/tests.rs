use std::path::{Path, PathBuf};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use diffo_core::{
    ChangeKind, CheckoutHistory, Commit, CommitFile, HeadState, RepositoryQueryId,
    RepositorySnapshot,
};
use diffo_ui::PaneSplit;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use super::{HistoryActivity, HistoryRequest, ReviewSelection};

const AREA: Rect = Rect::new(0, 0, 100, 30);
const PATCH: &str = concat!(
    "diff --git a/src/main.rs b/src/main.rs\n",
    "index 1111111..2222222 100644\n",
    "--- a/src/main.rs\n",
    "+++ b/src/main.rs\n",
    "@@ -1 +1 @@\n",
    "-fn old() {}\n",
    "+fn new() {}\n",
);
const FILE_PATCH: &str = concat!(
    "diff --git a/src/main.rs b/src/main.rs\n",
    "--- a/src/main.rs\n",
    "+++ b/src/main.rs\n",
    "@@ -1,3 +1,3 @@\n",
    " fn before() {}\n",
    "-fn old() {}\n",
    "+fn new() {}\n",
    " fn after() {}\n",
);
const TWO_FILE_PATCH: &str = concat!(
    "diff --git a/src/main.rs b/src/main.rs\n",
    "--- a/src/main.rs\n",
    "+++ b/src/main.rs\n",
    "@@ -1 +1 @@\n",
    "-fn first_old() {}\n",
    "+fn first_new() {}\n",
    "diff --git a/src/second.rs b/src/second.rs\n",
    "--- a/src/second.rs\n",
    "+++ b/src/second.rs\n",
    "@@ -1 +1 @@\n",
    "-fn second_old() {}\n",
    "+fn second_new() {}\n",
);

fn snapshot(head: &str) -> RepositorySnapshot {
    RepositorySnapshot {
        head: HeadState::Named {
            name: "main".to_owned(),
            commit: head.to_owned(),
        },
        ..RepositorySnapshot::default()
    }
}

fn commits() -> Vec<Commit> {
    vec![
        Commit {
            id: "aaaaaaaa".to_owned(),
            summary: "newest".to_owned(),
        },
        Commit {
            id: "bbbbbbbb".to_owned(),
            summary: "older".to_owned(),
        },
    ]
}

fn files() -> Vec<CommitFile> {
    vec![CommitFile {
        path: PathBuf::from("src/main.rs"),
        old_path: None,
        kind: ChangeKind::Modified,
    }]
}

fn two_files() -> Vec<CommitFile> {
    vec![
        files()[0].clone(),
        CommitFile {
            path: PathBuf::from("src/second.rs"),
            old_path: None,
            kind: ChangeKind::Modified,
        },
    ]
}

fn load_history(activity: &mut HistoryActivity) -> RepositoryQueryId {
    let Some(HistoryRequest::Commits { query_id }) = activity.take_request() else {
        panic!("expected history request");
    };
    assert!(activity.accept_history(
        query_id,
        CheckoutHistory {
            head_commit: Some("aaaaaaaa".to_owned()),
            commits: commits(),
        },
    ));
    query_id
}

fn accept_pending_patch(activity: &mut HistoryActivity) -> RepositoryQueryId {
    let Some(HistoryRequest::Patch {
        query_id,
        commit_id,
    }) = activity.take_request()
    else {
        panic!("expected patch request");
    };
    assert!(activity.accept_patch(query_id, &commit_id, PATCH.to_owned(), files()));
    query_id
}

fn install_pending_patch(activity: &mut HistoryActivity) {
    accept_pending_patch(activity);
    let preparation = activity.prepare_frame(AREA, PaneSplit::default());
    assert!(!preparation.preparing);
}

#[test]
fn history_and_patch_selection_commit_atomically() {
    let mut activity = HistoryActivity::new(&snapshot("aaaaaaaa"));
    load_history(&mut activity);

    assert!(activity.commits.is_empty());
    assert!(activity.selection.is_none());
    assert_eq!(
        activity
            .pending_selection
            .as_ref()
            .and_then(ReviewSelection::complete_change_id),
        Some("aaaaaaaa")
    );

    accept_pending_patch(&mut activity);
    assert!(activity.commits.is_empty());
    assert!(activity.selection.is_none());

    let preparation = activity.prepare_frame(AREA, PaneSplit::default());
    assert!(!preparation.preparing);
    assert_eq!(activity.commits, commits());
    assert_eq!(activity.files, files());
    assert_eq!(
        activity.selection,
        Some(ReviewSelection::HistoryFile {
            commit_id: "aaaaaaaa".to_owned(),
            path: PathBuf::from("src/main.rs"),
        })
    );
    assert_eq!(
        activity.document_commits(),
        (
            Some("aaaaaaaa".to_owned()),
            Some("aaaaaaaa".to_owned()),
            Some("aaaaaaaa".to_owned()),
        )
    );
}

#[test]
fn hunk_file_selection_stays_aggregate_and_r_loads_the_full_file_atomically() {
    let mut activity = HistoryActivity::new(&snapshot("aaaaaaaa"));
    load_history(&mut activity);
    install_pending_patch(&mut activity);

    assert!(
        activity.handle_file_outcome(diffo_ui::file_picker::Outcome::Selected(PathBuf::from(
            "src/main.rs"
        ),))
    );
    assert!(activity.take_request().is_none());
    let hunk_revision = activity
        .prepare_frame(AREA, PaneSplit::default())
        .content_revision;
    assert_eq!(activity.review.diff_view_mode, super::DiffViewMode::Hunk);

    assert_eq!(
        activity.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)),
            AREA,
            PaneSplit::default(),
        ),
        Some(super::HistoryEvent::Consumed)
    );
    let Some(HistoryRequest::File {
        query_id,
        commit_id,
        path,
        old_path,
    }) = activity.take_request()
    else {
        panic!("expected file request");
    };
    assert_eq!(commit_id, "aaaaaaaa");
    assert_eq!(path, Path::new("src/main.rs"));
    assert!(old_path.is_none());
    assert!(matches!(
        activity.selection,
        Some(ReviewSelection::HistoryFile { .. })
    ));
    assert_eq!(activity.review.diff_view_mode, super::DiffViewMode::Hunk);
    assert_eq!(
        activity
            .prepare_frame(AREA, PaneSplit::default())
            .content_revision,
        hunk_revision
    );

    assert!(activity.accept_file(query_id, &commit_id, &path, FILE_PATCH.to_owned()));
    assert_eq!(activity.review.diff_view_mode, super::DiffViewMode::Hunk);

    let preparation = activity.prepare_frame(AREA, PaneSplit::default());
    assert!(!preparation.preparing);
    assert_eq!(
        activity.selection,
        Some(ReviewSelection::HistoryFile {
            commit_id: "aaaaaaaa".to_owned(),
            path: PathBuf::from("src/main.rs"),
        })
    );
    assert_eq!(activity.review.diff_view_mode, super::DiffViewMode::Inline);
}

#[test]
fn selecting_another_history_file_jumps_within_the_same_aggregate_hunk() {
    let area = Rect::new(0, 0, 100, 10);
    let split = PaneSplit::default();
    let mut activity = HistoryActivity::new(&snapshot("aaaaaaaa"));
    load_history(&mut activity);
    let Some(HistoryRequest::Patch {
        query_id,
        commit_id,
    }) = activity.take_request()
    else {
        panic!("expected patch request");
    };
    assert!(activity.accept_patch(query_id, &commit_id, TWO_FILE_PATCH.to_owned(), two_files(),));
    let first = activity.prepare_frame(area, split);
    let revision = first.content_revision;

    assert!(
        activity.handle_file_outcome(diffo_ui::file_picker::Outcome::Selected(PathBuf::from(
            "src/second.rs"
        ),))
    );
    assert!(activity.take_request().is_none());
    let second = activity.prepare_frame(area, split);

    assert_eq!(second.content_revision, revision);
    assert!(activity.review.diff_scroll > 0);
    assert_eq!(
        activity.selection,
        Some(ReviewSelection::HistoryFile {
            commit_id: "aaaaaaaa".to_owned(),
            path: PathBuf::from("src/second.rs"),
        })
    );
}

#[test]
fn full_file_cache_is_scoped_to_the_commit() {
    let split = PaneSplit::default();
    let mut activity = HistoryActivity::new(&snapshot("aaaaaaaa"));
    load_history(&mut activity);
    install_pending_patch(&mut activity);

    assert!(activity.toggle_review_mode());
    let Some(HistoryRequest::File {
        query_id,
        commit_id,
        path,
        ..
    }) = activity.take_request()
    else {
        panic!("expected first file request");
    };
    assert!(activity.accept_file(
        query_id,
        &commit_id,
        &path,
        FILE_PATCH.replace("new", "first_commit"),
    ));
    activity.prepare_frame(AREA, split);

    assert!(
        activity.handle_commit_outcome(diffo_ui::file_picker::Outcome::Selected(
            "bbbbbbbb".to_owned(),
        ))
    );
    let Some(HistoryRequest::Patch {
        query_id,
        commit_id,
    }) = activity.take_request()
    else {
        panic!("expected second commit patch request");
    };
    assert!(activity.accept_patch(query_id, &commit_id, PATCH.to_owned(), files()));
    assert!(activity.toggle_review_mode());
    assert!(matches!(
        activity.take_request(),
        Some(HistoryRequest::File { commit_id, .. }) if commit_id == "bbbbbbbb"
    ));
}

#[test]
fn superseded_history_requests_do_not_leave_the_activity_preparing() {
    let split = PaneSplit::default();
    let mut activity = HistoryActivity::new(&snapshot("aaaaaaaa"));
    load_history(&mut activity);
    let Some(HistoryRequest::Patch {
        query_id,
        commit_id,
    }) = activity.take_request()
    else {
        panic!("expected initial patch request");
    };
    assert!(activity.accept_patch(query_id, &commit_id, TWO_FILE_PATCH.to_owned(), two_files(),));
    activity.prepare_frame(AREA, split);

    assert!(activity.toggle_review_mode());
    let Some(HistoryRequest::File {
        query_id,
        commit_id,
        path,
        ..
    }) = activity.take_request()
    else {
        panic!("expected file request");
    };
    assert!(
        activity.handle_file_outcome(diffo_ui::file_picker::Outcome::Selected(PathBuf::from(
            "src/second.rs"
        ),))
    );
    assert!(!activity.accept_file(query_id, &commit_id, &path, FILE_PATCH.to_owned()));
    activity.prepare_frame(AREA, split);
    assert!(!activity.is_preparing());

    assert!(
        activity.handle_commit_outcome(diffo_ui::file_picker::Outcome::Selected(
            "bbbbbbbb".to_owned(),
        ))
    );
    let Some(HistoryRequest::Patch {
        query_id,
        commit_id,
    }) = activity.take_request()
    else {
        panic!("expected superseded patch request");
    };
    assert!(
        !activity.handle_file_outcome(diffo_ui::file_picker::Outcome::Selected(PathBuf::from(
            "src/main.rs"
        ),))
    );
    assert!(activity.accept_patch(query_id, &commit_id, PATCH.to_owned(), files()));
    activity.prepare_frame(AREA, split);
    assert!(!activity.is_preparing());
}

#[test]
fn aggregate_commit_patch_is_split_in_file_order() {
    let patches = super::document::split_file_patches(TWO_FILE_PATCH);

    assert_eq!(patches.len(), 2);
    assert!(patches[0].contains("first_new"));
    assert!(!patches[0].contains("second_new"));
    assert!(patches[1].contains("second_new"));
}

#[test]
fn stale_history_patch_and_file_results_cannot_replace_committed_state() {
    let mut activity = HistoryActivity::new(&snapshot("aaaaaaaa"));
    let initial = load_history(&mut activity);
    install_pending_patch(&mut activity);

    activity.repository_changed(&snapshot("cccccccc"));
    assert!(!activity.accept_history(
        initial,
        CheckoutHistory {
            head_commit: Some("aaaaaaaa".to_owned()),
            commits: Vec::new(),
        }
    ));
    assert!(!activity.accept_patch(RepositoryQueryId(0), "bbbbbbbb", String::new(), Vec::new(),));
    assert!(!activity.accept_file(
        RepositoryQueryId(0),
        "aaaaaaaa",
        Path::new("src/main.rs"),
        String::new(),
    ));
    assert_eq!(
        activity.selection,
        Some(ReviewSelection::HistoryFile {
            commit_id: "aaaaaaaa".to_owned(),
            path: PathBuf::from("src/main.rs"),
        })
    );
}

#[test]
fn uppercase_shortcuts_are_rejected() {
    let mut activity = HistoryActivity::new(&snapshot("aaaaaaaa"));
    load_history(&mut activity);
    install_pending_patch(&mut activity);
    activity.prepare_frame(AREA, PaneSplit::default());

    for key in ['J', 'K', 'H', 'L', 'R', 'P', 'N', 'F'] {
        assert_eq!(
            activity.handle_event(
                &Event::Key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::SHIFT)),
                AREA,
                PaneSplit::default(),
            ),
            None
        );
    }
}

#[test]
fn clicking_a_commit_keeps_the_previous_picker_and_review_until_ready() {
    let split = PaneSplit::default();
    let mut activity = HistoryActivity::new(&snapshot("aaaaaaaa"));
    load_history(&mut activity);
    install_pending_patch(&mut activity);
    activity.prepare_frame(AREA, split);
    let commits = super::view::areas(AREA, split).commits;
    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: commits.x.saturating_add(2),
        row: commits.y.saturating_add(2),
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        activity.handle_event(&click, AREA, split),
        Some(super::HistoryEvent::Consumed)
    );
    activity.prepare_frame(AREA, split);

    assert_eq!(
        activity.commit_picker.selected().map(String::as_str),
        Some("aaaaaaaa")
    );
    assert_eq!(
        activity.selection,
        Some(ReviewSelection::HistoryFile {
            commit_id: "aaaaaaaa".to_owned(),
            path: PathBuf::from("src/main.rs"),
        })
    );
    assert_eq!(
        activity
            .pending_selection
            .as_ref()
            .and_then(ReviewSelection::complete_change_id),
        Some("bbbbbbbb")
    );
}

#[test]
fn renders_commit_and_file_pickers_with_compact_complete_change() {
    let area = Rect::new(0, 0, 80, 18);
    let split = PaneSplit::default();
    let mut activity = HistoryActivity::new(&snapshot("aaaaaaaa"));
    load_history(&mut activity);
    install_pending_patch(&mut activity);
    activity.prepare_frame(area, split);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| activity.render(frame, area, split))
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer());
}

#[test]
fn empty_commit_explains_that_it_has_no_file_changes() {
    let split = PaneSplit::default();
    let mut activity = HistoryActivity::new(&snapshot("aaaaaaaa"));
    load_history(&mut activity);
    let Some(HistoryRequest::Patch {
        query_id,
        commit_id,
    }) = activity.take_request()
    else {
        panic!("expected patch request");
    };
    assert!(activity.accept_patch(query_id, &commit_id, String::new(), Vec::new()));
    activity.prepare_frame(AREA, split);
    let backend = TestBackend::new(AREA.width, AREA.height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| activity.render(frame, AREA, split))
        .unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(rendered.contains("Commit contains no file changes."));
}
