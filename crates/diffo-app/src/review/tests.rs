use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use diffo_core::{ChangeKind, FileDiff, FileState, RepositorySnapshot};
use diffo_ui::PaneSplit;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use super::*;

mod stale;

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

fn key(character: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
}

fn left_click(column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn render(review: &ReviewActivity) -> (String, view::ReviewHitAreas) {
    let mut hits = None;
    let mut terminal = Terminal::new(TestBackend::new(45, 30)).unwrap();
    terminal
        .draw(|frame| {
            hits = Some(view::render_review(frame, frame.area(), review));
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    (text, hits.expect("review hit areas"))
}

fn render_text(review: &ReviewActivity) -> String {
    render(review).0
}

fn render_empty_diff_text(review: &ReviewActivity) -> String {
    let mut terminal = Terminal::new(TestBackend::new(60, 30)).unwrap();
    terminal
        .draw(|frame| view::render_empty_diff(frame, frame.area(), review))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

#[test]
fn generation_is_explicit_and_installs_only_known_targets() {
    let mut review = ReviewActivity::new(snapshot("new"), CodexAvailability::Available);
    assert!(review.active_request.is_none());

    let Some(ReviewEvent::Generate(request)) =
        review.handle_event(&enter(), Rect::new(0, 0, 100, 30), PaneSplit::default())
    else {
        panic!("generation request");
    };
    let id = ApplicationCommandId(1);
    review.generation_queued(id, request.clone());
    let first_target_id = request.first_target_id().to_owned();
    let result = request
        .validate_review(
            vec!["Overview".to_owned()],
            vec![ReviewStop {
                title: "Inspect behavior".to_owned(),
                category: AttentionCategory::Behavior,
                reason: "The behavior changes here.".to_owned(),
                target_id: first_target_id,
            }],
        )
        .unwrap();

    assert!(review.accept(ReviewCodexTaskResult {
        id,
        outcome: ReviewCodexOutcome::Generated(result),
    }));
    assert!(review.ready().is_some());
    assert_eq!(review.selected_stop, 0);
    assert!(review.pending_recenter);
}

#[test]
fn staging_from_review_keeps_the_path_and_rebinds_the_target() {
    let initial = snapshot("new");
    let request = ReviewRequest::from_snapshot(&initial).unwrap();
    let first_target_id = request.first_target_id().to_owned();
    let result = request
        .validate_review(
            vec!["Overview".to_owned()],
            vec![ReviewStop {
                title: "Inspect behavior".to_owned(),
                category: AttentionCategory::Behavior,
                reason: "The behavior changes here.".to_owned(),
                target_id: first_target_id,
            }],
        )
        .unwrap();
    let mut review = ReviewActivity::new(initial.clone(), CodexAvailability::Available);
    review.cached = Some(CachedReview {
        request,
        result,
        stale: false,
    });
    review.open_selected_stop();

    let event = Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert!(matches!(
        review.handle_event(&event, Rect::new(0, 0, 100, 30), PaneSplit::default()),
        Some(ReviewEvent::ToggleStage(file)) if file.area == crate::diff::ChangeArea::Unstaged
    ));
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
fn successful_staging_advances_to_the_next_review_step() {
    let initial = two_file_snapshot();
    let request = ReviewRequest::from_snapshot(&initial).unwrap();
    let ids = request.target_ids();
    let result = request
        .validate_review(
            vec!["Overview".to_owned()],
            ids.iter()
                .enumerate()
                .map(|(index, id)| ReviewStop {
                    title: format!("Stop {index}"),
                    category: AttentionCategory::Behavior,
                    reason: "Review this file.".to_owned(),
                    target_id: id.clone(),
                })
                .collect(),
        )
        .unwrap();
    let mut review = ReviewActivity::new(initial.clone(), CodexAvailability::Available);
    review.cached = Some(CachedReview {
        request,
        result,
        stale: false,
    });
    review.open_selected_stop();

    let mut staged = initial;
    staged.files[0].staged = staged.files[0].unstaged.take();
    review.repository_changed(staged);

    assert_eq!(review.selected_stop, 1);
    assert_eq!(
        review.selected_target().map(|target| target.id.as_str()),
        Some(ids[1].as_str())
    );
    assert_eq!(
        review.active_file().map(|file| file.area),
        Some(crate::diff::ChangeArea::Unstaged)
    );
}

#[test]
fn staging_advances_to_the_next_step_in_the_same_now_staged_file() {
    let initial = RepositorySnapshot {
        files: vec![FileState {
            path: "src/lib.rs".into(),
            old_path: None,
            kind: ChangeKind::Modified,
            staged: None,
            unstaged: Some(FileDiff {
                text: "@@ -1 +1 @@\n-old\n+first\n@@ -4 +4 @@\n-before\n+second\n".to_owned(),
            }),
        }],
        ..RepositorySnapshot::default()
    };
    let request = ReviewRequest::from_snapshot(&initial).unwrap();
    let ids = request.target_ids();
    let result = request
        .validate_review(
            vec!["Overview".to_owned()],
            ids.iter()
                .enumerate()
                .map(|(index, id)| ReviewStop {
                    title: format!("Step {index}"),
                    category: AttentionCategory::Behavior,
                    reason: "Review this change.".to_owned(),
                    target_id: id.clone(),
                })
                .collect(),
        )
        .unwrap();
    let mut review = ReviewActivity::new(initial.clone(), CodexAvailability::Available);
    review.cached = Some(CachedReview {
        request,
        result,
        stale: false,
    });
    review.open_selected_stop();

    let mut staged = initial;
    staged.files[0].staged = staged.files[0].unstaged.take();
    review.repository_changed(staged);

    assert_eq!(review.selected_stop, 1);
    assert_eq!(
        review.selected_target().map(|target| target.id.as_str()),
        Some(ids[1].as_str())
    );
    assert_eq!(
        review.active_file().map(|file| file.area),
        Some(crate::diff::ChangeArea::Staged)
    );
}

#[test]
fn keyboard_selection_immediately_opens_the_selected_step() {
    let initial = two_file_snapshot();
    let request = ReviewRequest::from_snapshot(&initial).unwrap();
    let ids = request.target_ids();
    let result = request
        .validate_review(
            vec!["Overview".to_owned()],
            ids.iter()
                .enumerate()
                .map(|(index, id)| ReviewStop {
                    title: format!("Step {index}"),
                    category: AttentionCategory::Behavior,
                    reason: "Review this change.".to_owned(),
                    target_id: id.clone(),
                })
                .collect(),
        )
        .unwrap();
    let mut review = ReviewActivity::new(initial, CodexAvailability::Available);
    review.cached = Some(CachedReview {
        request,
        result,
        stale: false,
    });
    review.open_selected_stop();
    let before = render_text(&review);

    assert!(matches!(
        review.handle_event(&key('n'), Rect::new(0, 0, 100, 30), PaneSplit::default()),
        Some(ReviewEvent::Redraw)
    ));
    assert_eq!(review.selected_stop, 1);
    assert_eq!(
        review.selected_target().map(|target| target.id.as_str()),
        Some(ids[1].as_str())
    );
    let after = render_text(&review);
    let row = |text: &str, needle: &str| {
        text.lines()
            .position(|line| line.contains(needle))
            .unwrap_or(usize::MAX)
    };
    for label in ["Review path", "1. Step 0", "2. Step 1"] {
        assert_eq!(row(&before, label), row(&after, label), "{label} moved");
    }
    assert!(row(&after, "Review path") < row(&after, "Step 2/2 · one change"));

    let _ = review.handle_event(&key('p'), Rect::new(0, 0, 100, 30), PaneSplit::default());
    assert_eq!(review.selected_stop, 0);
    assert_eq!(
        review.selected_target().map(|target| target.id.as_str()),
        Some(ids[0].as_str())
    );
}

#[test]
fn initial_screen_teaches_the_complete_workflow_without_internal_terms() {
    let review = ReviewActivity::new(snapshot("new"), CodexAvailability::Available);
    let text = format!(
        "{}{}",
        render_text(&review),
        render_empty_diff_text(&review)
    );

    assert!(text.contains("[ Generate (Enter) ]"));
    assert!(text.contains("How it works"));
    assert!(text.contains("n / p"));
    assert!(text.contains("click"));
    assert!(text.contains("stage or unstage"));
    assert!(text.contains("whole file"));
    assert!(text.contains("commit staged work"));
    assert!(text.contains("changes as they are now"));
    assert!(text.contains("marks it out of date"));
    let lower = text.to_lowercase();
    assert!(!lower.contains("hunk"));
    assert!(!lower.contains("review stop"));
    assert!(!lower.contains("review map"));
}

#[test]
fn only_the_visible_generate_button_starts_a_review() {
    let mut review = ReviewActivity::new(snapshot("new"), CodexAvailability::Available);
    let (_, hits) = render(&review);
    let button = hits.generate_area;
    review.generate_area = button;

    assert_eq!(
        button.width,
        u16::try_from("[ Generate (Enter) ]".len()).unwrap()
    );
    assert!(
        review
            .handle_event(
                &left_click(button.right(), button.y),
                Rect::new(0, 0, 100, 30),
                PaneSplit::default(),
            )
            .is_none()
    );
    assert!(matches!(
        review.handle_event(
            &left_click(button.x, button.y),
            Rect::new(0, 0, 100, 30),
            PaneSplit::default(),
        ),
        Some(ReviewEvent::Generate(_))
    ));
}

#[test]
fn ready_screen_connects_the_review_step_to_staging_and_commit_actions() {
    let initial = two_file_snapshot();
    let request = ReviewRequest::from_snapshot(&initial).unwrap();
    let ids = request.target_ids();
    let result = request
        .validate_review(
            vec!["Two files change the reviewed behavior.".to_owned()],
            ids.iter()
                .enumerate()
                .map(|(index, id)| ReviewStop {
                    title: format!("Inspect change {}", index + 1),
                    category: AttentionCategory::Behavior,
                    reason: "This change affects visible behavior.".to_owned(),
                    target_id: id.clone(),
                })
                .collect(),
        )
        .unwrap();
    let mut review = ReviewActivity::new(initial, CodexAvailability::Available);
    review.cached = Some(CachedReview {
        request,
        result,
        stale: false,
    });
    review.open_selected_stop();

    let text = render_text(&review);
    assert!(text.contains("Summary"));
    assert!(text.contains("Review path"));
    assert!(text.contains("Step 1/2 · one change"));
    assert!(text.contains("in a.rs"));
    assert!(text.contains("behavior · Unstaged"));
    assert!(text.contains("Why this matters"));
    assert!(text.contains("Stage file"));
    assert!(text.contains("Commit staged work"));
}

#[test]
fn clean_generating_stale_failed_and_unavailable_states_explain_the_next_action() {
    let clean = ReviewActivity::new(RepositorySnapshot::default(), CodexAvailability::Available);
    assert!(render_text(&clean).contains("Nothing to review"));

    let mut generating = ReviewActivity::new(snapshot("new"), CodexAvailability::Available);
    let Some(ReviewEvent::Generate(request)) =
        generating.handle_event(&enter(), Rect::new(0, 0, 100, 30), PaneSplit::default())
    else {
        panic!("generation request");
    };
    generating.generation_queued(ApplicationCommandId(1), request);
    assert!(render_text(&generating).contains("Building your review"));
    assert!(render_text(&generating).contains("Preparing changes and starting Codex"));
    assert!(render_text(&generating).contains("Esc  Cancel review"));
    assert!(
        generating
            .handle_event(&enter(), Rect::new(0, 0, 100, 30), PaneSplit::default())
            .is_none()
    );
    assert!(matches!(
        generating.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Rect::new(0, 0, 100, 30),
            PaneSplit::default(),
        ),
        Some(ReviewEvent::Cancel(ApplicationCommandId(1)))
    ));

    let initial = snapshot("new");
    let request = ReviewRequest::from_snapshot(&initial).unwrap();
    let result = request
        .validate_review(
            vec!["Overview".to_owned()],
            vec![ReviewStop {
                title: "Inspect behavior".to_owned(),
                category: AttentionCategory::Behavior,
                reason: "The behavior changes here.".to_owned(),
                target_id: request.first_target_id().to_owned(),
            }],
        )
        .unwrap();
    let mut stale = ReviewActivity::new(initial, CodexAvailability::Available);
    stale.cached = Some(CachedReview {
        request,
        result,
        stale: false,
    });
    stale.repository_changed(snapshot("newer"));
    assert!(render_text(&stale).contains("Regenerate"));

    let mut failed = ReviewActivity::new(snapshot("new"), CodexAvailability::Available);
    failed.failure = Some("Codex could not build this review.".to_owned());
    assert!(render_text(&failed).contains("Retry"));

    let unavailable = ReviewActivity::new(
        snapshot("new"),
        CodexAvailability::Unavailable("Install Codex, then restart Diffo.".to_owned()),
    );
    let text = format!(
        "{}{}",
        render_text(&unavailable),
        render_empty_diff_text(&unavailable)
    );
    assert!(text.contains("AI Review is unavailable"));
    assert!(text.contains("restart Diffo"));
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
    assert!(review.active_request.is_none());
}
