use std::path::PathBuf;

use diffo_core::{ChangeKind, FileDiff, FileState, HeadState};

use super::*;

fn queue_snapshot(diff: &str) -> RepositorySnapshot {
    RepositorySnapshot {
        files: vec![FileState {
            path: PathBuf::from("src/main.rs"),
            old_path: None,
            kind: ChangeKind::Modified,
            staged: Some(FileDiff {
                text: diff.to_owned(),
            }),
            unstaged: Some(FileDiff {
                text: "UNSTAGED".to_owned(),
            }),
        }],
        ..RepositorySnapshot::default()
    }
}

#[test]
fn stage_ai_commit_and_sync_can_be_entered_as_one_queue() {
    let mut initial = queue_snapshot("OLD STAGED");
    initial.files[0].staged = None;
    initial.head = HeadState::Named {
        name: "main".to_owned(),
        commit: "old-head".to_owned(),
    };
    initial.upstream = Some(diffo_core::UpstreamState {
        name: "origin/main".to_owned(),
        ahead: 0,
        behind: 0,
        recent_local_commits: Vec::new(),
    });
    let mut workbench = Workbench::new(initial);

    let _ = workbench.handle_events(
        &[
            key(KeyCode::Char('a')),
            key(KeyCode::Char('i')),
            key(KeyCode::Char('9')),
        ],
        Rect::new(0, 0, 100, 30),
    );

    assert_eq!(workbench.commands.queued_len(), 3);
    let stage = workbench
        .take_application_command(Instant::now())
        .expect("stage starts");
    assert_eq!(
        stage.action,
        ApplicationAction::Repository(RepositoryAction::StageAll)
    );

    let mut staged = queue_snapshot("NEW STAGED");
    staged.files[0].unstaged = None;
    staged.head = HeadState::Named {
        name: "main".to_owned(),
        commit: "old-head".to_owned(),
    };
    staged.upstream = Some(diffo_core::UpstreamState {
        name: "origin/main".to_owned(),
        ahead: 0,
        behind: 0,
        recent_local_commits: Vec::new(),
    });
    workbench.operation_completed(
        stage.id,
        RepositoryAction::StageAll,
        OperationResult::Stage,
        staged.clone(),
    );

    let ai = workbench
        .take_application_command(Instant::now())
        .expect("AI generation starts");
    let ApplicationAction::AiCommit(request) = &ai.action else {
        panic!("AI intent should resolve after staging");
    };
    assert_eq!(request.expected_staged[0].diff.text, "NEW STAGED");
    let handoff = workbench
        .ai_commit_finished(
            ai.id,
            AiCommitOutcome::Generated("feat: queue commands".to_owned()),
        )
        .expect("commit handoff");
    let mut committed = staged;
    committed.files.clear();
    committed.head = HeadState::Named {
        name: "main".to_owned(),
        commit: "new-head".to_owned(),
    };
    workbench.operation_completed(
        ai.id,
        handoff.action,
        OperationResult::Commit {
            hash: "new-head".to_owned(),
        },
        committed,
    );

    let sync = workbench
        .take_application_command(Instant::now())
        .expect("sync starts after commit");
    assert_eq!(
        sync.action,
        ApplicationAction::Repository(RepositoryAction::Sync)
    );
}

#[test]
fn repeated_stage_all_shortcuts_resolve_one_turn_at_a_time() {
    let mut initial = queue_snapshot("UNSTAGED");
    initial.files[0].unstaged = initial.files[0].staged.take();
    let mut workbench = Workbench::new(initial);

    let _ = workbench.handle_events(
        &[key(KeyCode::Char('a')), key(KeyCode::Char('a'))],
        Rect::new(0, 0, 100, 30),
    );

    let stage = workbench
        .take_application_command(Instant::now())
        .expect("stage starts");
    let mut staged = queue_snapshot("STAGED");
    staged.files[0].unstaged = None;
    workbench.operation_completed(
        stage.id,
        RepositoryAction::StageAll,
        OperationResult::Stage,
        staged,
    );

    let unstage = workbench
        .take_application_command(Instant::now())
        .expect("second toggle starts");
    assert_eq!(
        unstage.action,
        ApplicationAction::Repository(RepositoryAction::UnstageAll)
    );
}

#[test]
fn manual_commit_can_be_queued_before_staging_finishes() {
    let mut initial = queue_snapshot("UNSTAGED");
    initial.files[0].unstaged = initial.files[0].staged.take();
    let mut workbench = Workbench::new(initial);

    let _ = workbench.handle_events(
        &[key(KeyCode::Char('a')), key(KeyCode::Enter)],
        Rect::new(0, 0, 100, 30),
    );
    let stage = workbench
        .take_application_command(Instant::now())
        .expect("stage starts");
    let mut staged = queue_snapshot("STAGED");
    staged.files[0].unstaged = None;
    workbench.operation_completed(
        stage.id,
        RepositoryAction::StageAll,
        OperationResult::Stage,
        staged,
    );

    let commit = workbench
        .take_application_command(Instant::now())
        .expect("commit starts");
    assert_eq!(
        commit.action,
        ApplicationAction::Repository(RepositoryAction::Commit("Update 1 file".to_owned()))
    );
}

#[test]
fn generation_failure_cancels_every_command_behind_it() {
    let mut workbench = Workbench::new(queue_snapshot("STAGED"));
    workbench.request_ai_commit();
    workbench.commands.enqueue_intent(CommandIntent::Sync);
    let command = workbench
        .take_application_command(Instant::now())
        .expect("AI command");

    let _ = workbench.ai_commit_finished(
        command.id,
        AiCommitOutcome::Failed("not authenticated".to_owned()),
    );

    assert!(!workbench.commands.has_work());
}

#[test]
fn queue_controls_truncate_waiting_work_and_cancel_the_active_command() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    workbench.commands.enqueue(RepositoryAction::Fetch);
    let running = workbench
        .take_application_command(Instant::now())
        .expect("fetch starts");
    let sync = workbench.commands.enqueue(RepositoryAction::Sync);
    workbench.commands.enqueue_update();
    let area = Rect::new(0, 0, 100, 30);
    let content = workbench_areas(area).content;
    let cancel_sync = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: content.right().saturating_sub(3),
        row: content.y.saturating_add(3),
        modifiers: KeyModifiers::NONE,
    });

    let _ = workbench.handle_event(&cancel_sync, area);

    assert_eq!(workbench.commands.queued_len(), 0);
    assert_eq!(
        workbench.commands.active().map(|command| command.id),
        Some(running.id)
    );
    assert!(!workbench.commands.entries().any(|(id, _, _)| id == sync));

    workbench.commands.enqueue(RepositoryAction::Sync);
    let cancel_all = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: content.right().saturating_sub(5),
        row: content.y.saturating_add(4),
        modifiers: KeyModifiers::NONE,
    });
    let _ = workbench.handle_event(&cancel_all, area);

    assert!(running.cancellation.is_cancelled());
    assert_eq!(workbench.commands.queued_len(), 0);
    assert_eq!(
        workbench.commands.active().map(|command| command.state),
        Some(CommandState::Cancelling)
    );
}

#[test]
fn preparation_failure_discards_every_waiting_command() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    workbench.commands.enqueue_intent(CommandIntent::StageAll);
    workbench.commands.enqueue_update();

    assert!(workbench.take_application_command(Instant::now()).is_none());

    assert!(!workbench.commands.has_work());
    assert!(matches!(
        workbench.modal,
        Some(Modal::Error(ref error))
            if error.title == "Queued command stopped"
                && error.detail.contains("no longer has the changes")
    ));
}
