use super::*;

#[test]
fn sync_keys_run_once_and_disable_together_in_every_activity() {
    let area = Rect::new(0, 0, 100, 30);
    for activity in [Activity::Diff, Activity::Explorer, Activity::Search] {
        for shortcut in [KeyCode::Char('9'), KeyCode::F(9)] {
            let mut workbench = Workbench::new(RepositorySnapshot::default());
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
    let snapshot = RepositorySnapshot {
        head: diffo_core::HeadState::Named {
            name: "main".to_owned(),
            commit: "123456789abcdef".to_owned(),
        },
        ..RepositorySnapshot::default()
    };
    let area = Rect::new(0, 0, 100, 30);
    let status = tool_areas(workbench_areas(area).content).status;
    let sync_column = status
        .right()
        .saturating_sub(u16::try_from("[ Sync (9 / F9) ]".len()).unwrap());

    for activity in [Activity::Diff, Activity::Explorer, Activity::Search] {
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
    for activity in [Activity::Diff, Activity::Explorer] {
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
