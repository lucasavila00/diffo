use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use diffo_core::{CheckoutHistory, Commit, HeadState, RepositoryQueryId, RepositorySnapshot};
use diffo_highlight::SyntaxHighlighter;
use diffo_ui::PaneSplit;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use super::{HistoryActivity, HistoryRequest, prepare};

const PATCH: &str = concat!(
    "diff --git a/src/main.rs b/src/main.rs\n",
    "--- a/src/main.rs\n",
    "+++ b/src/main.rs\n",
    "@@ -1 +1 @@\n",
    "-fn old() {}\n",
    "+fn new() {}\n",
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

fn install_pending_patch(activity: &mut HistoryActivity) {
    let Some(HistoryRequest::Patch {
        query_id,
        commit_id,
    }) = activity.take_request()
    else {
        panic!("expected patch request");
    };
    assert!(activity.accept_patch(query_id, commit_id.clone(), PATCH.to_owned()));
    let outcome = prepare::prepare(
        prepare::PrepareRequest {
            id: activity.latest_prepare,
            commit_id,
            summary: activity.commit_summary("aaaaaaaa").unwrap().to_owned(),
            patch: PATCH.into(),
            target_scroll: 0,
            viewport_rows: 20,
            window_viewports: 3,
        },
        &SyntaxHighlighter::new(),
    );
    assert!(activity.install_prepared(outcome));
}

#[test]
fn history_and_patch_selection_commit_atomically() {
    let mut activity = HistoryActivity::new(&snapshot("aaaaaaaa"));
    load_history(&mut activity);

    assert!(activity.commits.is_empty());
    assert!(activity.patch.is_none());
    assert_eq!(
        activity
            .pending_selection
            .as_ref()
            .and_then(super::ReviewSelection::complete_change_id),
        Some("aaaaaaaa")
    );

    install_pending_patch(&mut activity);

    assert_eq!(activity.commits, commits());
    assert_eq!(
        activity
            .patch
            .as_ref()
            .map(|patch| patch.commit_id.as_str()),
        Some("aaaaaaaa")
    );
    assert_eq!(
        activity
            .selection
            .as_ref()
            .and_then(super::ReviewSelection::complete_change_id),
        Some("aaaaaaaa")
    );
}

#[test]
fn stale_history_and_patch_results_cannot_replace_committed_state() {
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
    assert!(!activity.accept_patch(RepositoryQueryId(0), "bbbbbbbb".to_owned(), String::new()));
    assert_eq!(
        activity
            .patch
            .as_ref()
            .map(|patch| patch.commit_id.as_str()),
        Some("aaaaaaaa")
    );
}

#[test]
fn uppercase_shortcuts_are_rejected() {
    let mut activity = HistoryActivity::new(&snapshot("aaaaaaaa"));
    load_history(&mut activity);
    install_pending_patch(&mut activity);
    activity.prepare_frame(Rect::new(0, 0, 100, 30), PaneSplit::default());

    for key in ['J', 'K', 'L', 'F'] {
        assert_eq!(
            activity.handle_event(
                &Event::Key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::SHIFT)),
                Rect::new(0, 0, 100, 30),
                PaneSplit::default(),
            ),
            None
        );
    }
}

#[test]
fn clicking_a_commit_keeps_the_previous_row_and_patch_until_ready() {
    let area = Rect::new(0, 0, 100, 30);
    let split = PaneSplit::default();
    let mut activity = HistoryActivity::new(&snapshot("aaaaaaaa"));
    load_history(&mut activity);
    install_pending_patch(&mut activity);
    activity.prepare_frame(area, split);
    let commits = super::view::areas(area, split).commits;
    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: commits.x.saturating_add(2),
        row: commits.y.saturating_add(2),
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        activity.handle_event(&click, area, split),
        Some(super::HistoryEvent::Consumed)
    );
    activity.prepare_frame(area, split);

    assert_eq!(
        activity.picker.selected().map(String::as_str),
        Some("aaaaaaaa")
    );
    assert_eq!(
        activity
            .patch
            .as_ref()
            .map(|patch| patch.commit_id.as_str()),
        Some("aaaaaaaa")
    );
    assert_eq!(
        activity
            .pending_selection
            .as_ref()
            .and_then(super::ReviewSelection::complete_change_id),
        Some("bbbbbbbb")
    );
}

#[test]
fn renders_flat_history_and_hunk_only_commit_diff() {
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
