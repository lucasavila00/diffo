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

fn two_file_snapshot() -> RepositorySnapshot {
    RepositorySnapshot {
        files: ["a.rs", "b.rs"]
            .into_iter()
            .map(|path| FileState {
                path: path.into(),
                old_path: None,
                kind: ChangeKind::Modified,
                staged: None,
                unstaged: Some(FileDiff {
                    text: format!("@@ -1 +1 @@\n-old\n+{path}\n"),
                }),
            })
            .collect(),
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

    assert!(
        review
            .handle_event(&enter(), Rect::new(0, 0, 100, 30), PaneSplit::default())
            .is_some()
    );
    let task = review.take_task().expect("generation task");
    let request = &task.request;
    let first_hunk_id = request.first_hunk_id().to_owned();
    let result = request
        .validate_review(
            vec!["Overview".to_owned()],
            vec![ReviewStop {
                title: "Inspect behavior".to_owned(),
                category: AttentionCategory::Behavior,
                reason: "The behavior changes here.".to_owned(),
                primary_hunk_id: first_hunk_id.clone(),
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
fn staging_from_review_keeps_the_map_and_rebinds_the_hunk() {
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
                primary_hunk_id: first_hunk_id.clone(),
            }],
        )
        .unwrap();
    let mut review = ReviewActivity::new(initial.clone(), CodexAvailability::Available);
    review.cached = Some(CachedReview { request, result });
    review.active_hunk_id = Some(first_hunk_id.clone());

    let event = Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert!(matches!(
        review.handle_event(&event, Rect::new(0, 0, 100, 30), PaneSplit::default()),
        Some(ReviewEvent::ToggleStage(file)) if file.area == crate::diff::ChangeArea::Unstaged
    ));
    assert_eq!(
        review.active_hunk_id.as_deref(),
        Some(first_hunk_id.as_str())
    );

    let mut staged = initial;
    staged.files[0].staged = staged.files[0].unstaged.take();
    review.repository_changed(staged);

    assert!(review.ready().is_some());
    assert_eq!(
        review.active_file().map(|file| file.area),
        Some(crate::diff::ChangeArea::Staged)
    );
}

#[test]
fn successful_staging_advances_to_the_next_unstaged_stop() {
    let initial = two_file_snapshot();
    let request = ReviewRequest::from_snapshot(&initial).unwrap();
    let ids = request.hunk_ids();
    let result = request
        .validate_review(
            vec!["Overview".to_owned()],
            ids.iter()
                .enumerate()
                .map(|(index, id)| ReviewStop {
                    title: format!("Stop {index}"),
                    category: AttentionCategory::Behavior,
                    reason: "Review this file.".to_owned(),
                    primary_hunk_id: id.clone(),
                })
                .collect(),
        )
        .unwrap();
    let mut review = ReviewActivity::new(initial.clone(), CodexAvailability::Available);
    review.cached = Some(CachedReview { request, result });
    review.active_hunk_id = Some(ids[0].clone());

    let mut staged = initial;
    staged.files[0].staged = staged.files[0].unstaged.take();
    review.repository_changed(staged);

    assert_eq!(review.selected_stop, 1);
    assert_eq!(review.active_hunk_id.as_deref(), Some(ids[1].as_str()));
    assert_eq!(
        review.active_file().map(|file| file.area),
        Some(crate::diff::ChangeArea::Unstaged)
    );
}

#[test]
fn review_exposes_the_existing_ai_commit_action() {
    let mut review = ReviewActivity::new(snapshot("new"), CodexAvailability::Available);
    let event = Event::Key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
    assert!(matches!(
        review.handle_event(&event, Rect::new(0, 0, 100, 30), PaneSplit::default()),
        Some(ReviewEvent::AiCommit)
    ));
}

#[test]
fn unavailable_codex_never_creates_a_task() {
    let mut review = ReviewActivity::new(
        snapshot("new"),
        CodexAvailability::Unavailable("Codex is missing".to_owned()),
    );

    assert!(
        review
            .handle_event(&enter(), Rect::new(0, 0, 100, 30), PaneSplit::default())
            .is_none()
    );
    assert!(review.take_task().is_none());
}
