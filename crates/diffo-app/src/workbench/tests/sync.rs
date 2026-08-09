use super::*;

fn tracked_snapshot() -> RepositorySnapshot {
    RepositorySnapshot {
        head: diffo_core::HeadState::Named {
            name: "main".to_owned(),
            commit: "123456789abcdef".to_owned(),
        },
        upstream: Some(diffo_core::UpstreamState {
            name: "origin/main".to_owned(),
            ahead: 0,
            behind: 0,
            recent_local_commits: Vec::new(),
        }),
        ..RepositorySnapshot::default()
    }
}

#[test]
fn sync_keys_run_once_and_disable_together_in_every_activity() {
    let area = Rect::new(0, 0, 100, 30);
    for activity in [Activity::Diff, Activity::Explorer, Activity::Review] {
        for shortcut in [KeyCode::Char('9'), KeyCode::F(9)] {
            let mut workbench = Workbench::new(tracked_snapshot());
            workbench.active = activity;

            let _ = workbench.handle_event(&key(shortcut), area);
            let _ = workbench.handle_event(&key(shortcut), area);

            assert_eq!(workbench.commands.queued_len(), 1);
            assert!(!workbench.diff.model.sync_enabled());
        }
    }
}

#[test]
fn shared_footer_sync_button_runs_the_same_action() {
    let snapshot = tracked_snapshot();
    let area = Rect::new(0, 0, 100, 30);
    let status = tool_areas(workbench_areas(area).content).status;
    let sync_column = status
        .right()
        .saturating_sub(u16::try_from("[ Sync (9 / F9) ]".len()).unwrap());

    for activity in [Activity::Diff, Activity::Explorer, Activity::Review] {
        let mut workbench = Workbench::new(snapshot.clone());
        workbench.active = activity;
        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: sync_column,
            row: status.y,
            modifiers: KeyModifiers::NONE,
        });

        let _ = workbench.handle_event(&click, area);
        let _ = workbench.handle_event(&click, area);

        assert_eq!(workbench.commands.queued_len(), 1);
        assert!(!workbench.diff.model.sync_enabled());
    }
}

#[test]
fn missing_upstream_sync_selects_origin_and_queues_the_same_sync_operation() {
    let area = Rect::new(0, 0, 100, 30);
    let mut snapshot = tracked_snapshot();
    snapshot.upstream = None;
    let mut workbench = Workbench::new(snapshot);

    let _ = workbench.handle_event(&key(KeyCode::F(9)), area);
    assert!(matches!(workbench.modal, Some(Modal::SyncRemotePicker(_))));
    assert_eq!(
        workbench.take_sync_remote_query(),
        Some(diffo_core::RepositoryQueryId(1))
    );

    workbench.sync_remotes_loaded(
        diffo_core::RepositoryQueryId(1),
        vec!["backup".to_owned(), "origin".to_owned()],
    );

    assert!(workbench.modal.is_none());
    assert_eq!(workbench.commands.queued_len(), 1);
    assert_eq!(
        workbench.diff.model.network_operation(),
        Some(crate::diff::NetworkOperation::Sync)
    );
    assert_eq!(
        workbench.commands.start_next().unwrap().action,
        ApplicationAction::Repository(RepositoryAction::SyncToRemote("origin".to_owned()))
    );
}

#[test]
fn missing_upstream_sync_uses_a_picker_only_for_ambiguous_non_origin_remotes() {
    let area = Rect::new(0, 0, 100, 30);
    let mut snapshot = tracked_snapshot();
    snapshot.upstream = None;
    let mut workbench = Workbench::new(snapshot);

    let _ = workbench.handle_event(&key(KeyCode::Char('9')), area);
    let query_id = workbench.take_sync_remote_query().unwrap();
    workbench.sync_remotes_loaded(query_id, vec!["alpha".to_owned(), "beta".to_owned()]);
    assert!(matches!(workbench.modal, Some(Modal::SyncRemotePicker(_))));

    let _ = workbench.handle_event(&key(KeyCode::Enter), area);

    assert!(workbench.modal.is_none());
    assert_eq!(
        workbench.commands.start_next().unwrap().action,
        ApplicationAction::Repository(RepositoryAction::SyncToRemote("alpha".to_owned()))
    );
}

#[test]
fn missing_upstream_sync_reports_no_remote_without_queuing_work() {
    let area = Rect::new(0, 0, 100, 30);
    let mut snapshot = tracked_snapshot();
    snapshot.upstream = None;
    let mut workbench = Workbench::new(snapshot);

    let _ = workbench.handle_event(&key(KeyCode::F(9)), area);
    let query_id = workbench.take_sync_remote_query().unwrap();
    workbench.sync_remotes_loaded(query_id, Vec::new());

    assert!(matches!(
        workbench.modal,
        Some(Modal::Error(ref error))
            if error.title == "Sync failed"
                && error.detail == "No remotes are configured; Sync does not create remotes"
    ));
    assert_eq!(workbench.commands.queued_len(), 0);
    assert!(workbench.toasts.as_slice().is_empty());
}

#[test]
fn shared_footer_command_and_help_buttons_open_their_modals() {
    let area = Rect::new(0, 0, 100, 30);
    let status = tool_areas(workbench_areas(area).content).status;
    let controls = "[ Commands (1 / F1) ] [ Help (2 / F2) ] [ Sync (9 / F9) ]";
    let commands_column = status
        .right()
        .saturating_sub(u16::try_from(controls.len()).unwrap());
    let help_column =
        commands_column.saturating_add(u16::try_from("[ Commands (1 / F1) ] ".len()).unwrap());
    let click = |column| {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row: status.y,
            modifiers: KeyModifiers::NONE,
        })
    };
    let mut workbench = Workbench::new(RepositorySnapshot::default());

    let _ = workbench.handle_event(&click(commands_column), area);
    assert!(matches!(workbench.modal, Some(Modal::CommandPalette(_))));
    workbench.close_modal();

    let _ = workbench.handle_event(&click(help_column), area);
    assert!(matches!(workbench.modal, Some(Modal::Help)));
}

#[test]
fn operation_toasts_render_in_diff_and_explorer() {
    let mut rendered = Vec::new();
    for activity in [Activity::Diff, Activity::Explorer, Activity::Review] {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.active = activity;
        assert_eq!(
            workbench
                .diff
                .model
                .start_repository_action(RepositoryAction::Sync),
            Some(RepositoryAction::Sync)
        );
        let id = workbench.commands.enqueue(RepositoryAction::Sync);
        let _ = workbench.commands.start_next();
        workbench.operation_completed(
            id,
            RepositoryAction::Sync,
            OperationResult::Sync {
                plan: Box::new(SyncPlan {
                    branch: "main".to_owned(),
                    upstream: "origin/main".to_owned(),
                    local_only: 0,
                    upstream_only: 1,
                    establish_upstream: false,
                }),
            },
            RepositorySnapshot::default(),
        );
        assert_eq!(workbench.diff.model.network_operation(), None);
        assert_eq!(
            workbench.toasts.as_slice()[0].title,
            "Fast-forwarded main by 1 commit."
        );
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| workbench.render(frame)).unwrap();
        rendered.push((
            activity,
            buffer_region(terminal.backend().buffer(), Rect::new(55, 25, 44, 3)),
        ));
    }
    insta::assert_debug_snapshot!(rendered);
}
