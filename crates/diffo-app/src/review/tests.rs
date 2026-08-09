use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use diffo_core::{ChangeKind, FileDiff, FileState, RepositorySnapshot};
use diffo_ui::PaneSplit;
use ratatui::layout::Rect;

use super::*;

fn snapshot(text: &str) -> RepositorySnapshot {
    RepositorySnapshot {
        files: vec![FileState {
            path: "src/lib.rs".into(),
            old_path: None,
            kind: ChangeKind::Modified,
            staged: None,
            unstaged: Some(FileDiff {
                text: format!("@@ -1 +1 @@\n-old\n+{text}\n"),
            }),
        }],
        ..RepositorySnapshot::default()
    }
}

fn enter() -> Event {
    Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
}

#[test]
fn generation_is_explicit_and_installs_only_known_hunks() {
    let mut review = ReviewActivity::new(snapshot("new"), CodexAvailability::Available);
    assert!(review.take_task().is_none());

    assert!(review.handle_event(&enter(), Rect::new(0, 0, 100, 30), PaneSplit::default()));
    let task = review.take_task().expect("generation task");
    let request = match &task.request {
        ReviewCodexRequest::Generate(request) => request,
        ReviewCodexRequest::Ask(_) => panic!("expected generation"),
    };
    let first_hunk_id = request.first_hunk_id().to_owned();
    let result = request
        .validate_review(
            vec!["Overview".to_owned()],
            vec![ReviewStop {
                title: "Inspect behavior".to_owned(),
                category: AttentionCategory::Behavior,
                reason: "The behavior changes here.".to_owned(),
                primary_hunk_id: first_hunk_id.clone(),
                related_hunk_ids: Vec::new(),
            }],
        )
        .unwrap();

    assert!(review.accept(ReviewCodexTaskResult {
        id: task.id,
        outcome: ReviewCodexOutcome::Generated(result),
    }));
    assert!(review.ready().is_some());
    assert_eq!(
        review.active_hunk_id.as_deref(),
        Some(first_hunk_id.as_str())
    );
}

#[test]
fn repository_change_makes_a_ready_review_stale() {
    let initial = snapshot("new");
    let request = ReviewRequest::from_snapshot(&initial).unwrap();
    let first_hunk_id = request.first_hunk_id().to_owned();
    let result = request
        .validate_review(
            vec!["Overview".to_owned()],
            vec![ReviewStop {
                title: "Inspect behavior".to_owned(),
                category: AttentionCategory::Behavior,
                reason: "The behavior changes here.".to_owned(),
                primary_hunk_id: first_hunk_id,
                related_hunk_ids: Vec::new(),
            }],
        )
        .unwrap();
    let mut review = ReviewActivity::new(initial, CodexAvailability::Available);
    review.cached = Some(CachedReview { request, result });

    review.repository_changed(snapshot("newer"));

    assert!(review.stale());
    assert!(review.ready().is_none());
}

#[test]
fn unavailable_codex_never_creates_a_task() {
    let mut review = ReviewActivity::new(
        snapshot("new"),
        CodexAvailability::Unavailable("Codex is missing".to_owned()),
    );

    assert!(!review.handle_event(&enter(), Rect::new(0, 0, 100, 30), PaneSplit::default()));
    assert!(review.take_task().is_none());
}
