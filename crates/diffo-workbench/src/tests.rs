use super::*;
use crossterm::event::{KeyEvent, KeyEventState, MouseEvent};
use diffo_app::NetworkOperation;
use diffo_explorer::COLLAPSE_ALL_COMMAND;
use diffo_ui::theme;
use ratatui::{Terminal, backend::TestBackend, style::Modifier};

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn start_repository_command(
    workbench: &mut Workbench,
    action: RepositoryAction,
) -> ApplicationCommandId {
    let id = workbench.commands.enqueue(action);
    assert_eq!(
        workbench
            .take_repository_command()
            .map(|command| command.id),
        Some(id)
    );
    id
}

#[test]
fn tab_cycles_activities_without_changing_diff_state() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    workbench.diff.model.diff_scroll = 17;
    let tab = Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    let area = Rect::new(0, 0, 100, 30);

    let _ = workbench.handle_event(&tab, area);
    assert_eq!(workbench.active, Activity::Explorer);
    let _ = workbench.handle_event(&tab, area);
    assert_eq!(workbench.active, Activity::Search);
    let _ = workbench.handle_event(&tab, area);
    assert_eq!(workbench.active, Activity::Diff);
    assert_eq!(workbench.diff.model.diff_scroll, 17);
}

#[test]
fn activity_bar_click_selects_and_consumes_the_activity() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 2,
        row: 4,
        modifiers: KeyModifiers::NONE,
    });

    let _ = workbench.handle_event(&click, Rect::new(0, 0, 100, 30));
    assert_eq!(workbench.active, Activity::Search);
}

#[test]
fn pane_drag_is_shared_across_activities() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let area = Rect::new(0, 0, 100, 30);
    let pane_area = tool_areas(workbench_areas(area).content).content;
    let seam = workbench.pane_split.areas(pane_area).trailing.x;
    let mouse = |kind, column| {
        Event::Mouse(MouseEvent {
            kind,
            column,
            row: pane_area.y.saturating_add(2),
            modifiers: KeyModifiers::NONE,
        })
    };

    let _ = workbench.handle_event(&mouse(MouseEventKind::Down(MouseButton::Left), seam), area);
    let _ = workbench.handle_event(&mouse(MouseEventKind::Drag(MouseButton::Left), 62), area);
    let _ = workbench.handle_event(&mouse(MouseEventKind::Up(MouseButton::Left), 62), area);

    assert_eq!(workbench.pane_split.percent(), 60);
    assert_eq!(workbench.diff.model.file_pane_percent, 60);
    let _ = workbench.handle_event(&key(KeyCode::Tab), area);
    assert_eq!(workbench.active, Activity::Explorer);
    assert_eq!(workbench.pane_split.areas(pane_area).trailing.x, 62);
    let _ = workbench.handle_event(&key(KeyCode::Tab), area);
    assert_eq!(workbench.active, Activity::Search);
    assert_eq!(workbench.pane_split.areas(pane_area).trailing.x, 62);
}

#[test]
fn pane_toggle_is_global_and_diff_overlays_capture_input() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let area = Rect::new(0, 0, 100, 30);

    let _ = workbench.handle_event(&key(KeyCode::Tab), area);
    let _ = workbench.handle_event(&key(KeyCode::Char('e')), area);
    assert_eq!(workbench.pane_split.percent(), 0);
    let _ = workbench.handle_event(&key(KeyCode::Char('e')), area);
    assert_eq!(workbench.pane_split.percent(), 25);

    workbench.active = Activity::Diff;
    workbench.diff.model.help_open = true;
    let _ = workbench.handle_event(&key(KeyCode::Char('e')), area);
    assert_eq!(workbench.pane_split.percent(), 25);
}

#[test]
fn explorer_picker_menu_captures_global_shortcuts() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let area = Rect::new(0, 0, 100, 30);
    workbench.active = Activity::Explorer;
    workbench.explorer.accept(ExplorerOutcome::Paths {
        id: 1,
        result: Ok(vec![std::path::PathBuf::from("file.txt")]),
    });
    workbench.prepare_frame(area);
    let pane_area = tool_areas(workbench_areas(area).content).content;
    let tree = workbench.pane_split.areas(pane_area).leading;
    let right_click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: tree.x.saturating_add(2),
        row: tree.y.saturating_add(1),
        modifiers: KeyModifiers::NONE,
    });

    let _ = workbench.handle_event(&right_click, area);
    assert!(workbench.explorer.has_open_picker_menu());

    for code in [KeyCode::Char('e'), KeyCode::Char('1'), KeyCode::Char('q')] {
        let _ = workbench.handle_event(&key(code), area);
    }
    assert_eq!(workbench.pane_split.percent(), 25);
    assert!(!workbench.active_palette().is_open());
    assert!(!workbench.should_quit());
    assert!(workbench.explorer.has_open_picker_menu());

    let _ = workbench.handle_event(&key(KeyCode::Esc), area);
    assert!(!workbench.explorer.has_open_picker_menu());
    assert!(!workbench.should_quit());
}

#[test]
fn tab_requires_an_unmodified_key_press() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let repeat = Event::Key(crossterm::event::KeyEvent {
        code: KeyCode::Tab,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Repeat,
        state: KeyEventState::NONE,
    });
    let modified = Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT));

    let _ = workbench.handle_event(&repeat, Rect::default());
    let _ = workbench.handle_event(&modified, Rect::default());
    assert_eq!(workbench.active, Activity::Diff);
}

#[test]
fn empty_activities_keep_quit_available() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    workbench.active = Activity::Explorer;
    let quit = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));

    let _ = workbench.handle_event(&quit, Rect::default());
    assert!(workbench.should_quit());
}

#[test]
fn empty_search_draws_the_shared_page_panes() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    workbench.active = Activity::Search;
    let backend = TestBackend::new(20, 12);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|frame| workbench.render(frame)).unwrap();

    let pane_area = tool_areas(workbench_areas(Rect::new(0, 0, 20, 12)).content).content;
    let seam = workbench.pane_split.areas(pane_area).trailing.x;
    assert_eq!(
        terminal.backend().buffer()[(seam, pane_area.y)].symbol(),
        "┌"
    );
    let marker = workbench.pane_split.seam_marker_area(pane_area);
    let marker_cell = &terminal.backend().buffer()[(marker.x, marker.y)];
    assert_eq!(marker_cell.symbol(), interaction::PANE_DRAG);
    assert_eq!(marker_cell.fg, theme::TEXT);
    assert!(marker_cell.modifier.contains(Modifier::BOLD));
    assert!(
        workbench
            .pane_split
            .contains_seam(pane_area, marker.x, marker.y)
    );
}

#[test]
fn palettes_keep_separate_state_for_each_activity() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let area = Rect::new(0, 0, 100, 30);

    let _ = workbench.handle_event(&key(KeyCode::Char('1')), area);
    let _ = workbench.handle_event(&key(KeyCode::Char('p')), area);
    let _ = workbench.handle_event(&key(KeyCode::Tab), area);
    let _ = workbench.handle_event(&key(KeyCode::Char('1')), area);
    let _ = workbench.handle_event(&key(KeyCode::Char('c')), area);
    let _ = workbench.handle_event(&key(KeyCode::Tab), area);
    let _ = workbench.handle_event(&key(KeyCode::Tab), area);

    assert_eq!(workbench.active, Activity::Diff);
    assert_eq!(workbench.active_palette().query(), "p");
    let _ = workbench.handle_event(&key(KeyCode::Tab), area);
    assert_eq!(workbench.active_palette().query(), "c");
    assert!(
        workbench
            .active_palette()
            .matches()
            .iter()
            .any(|command| command.id == COLLAPSE_ALL_COMMAND)
    );
}

#[test]
fn shared_git_commands_execute_from_every_activity() {
    let area = Rect::new(0, 0, 100, 30);
    for activity in [Activity::Diff, Activity::Explorer, Activity::Search] {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.active = activity;

        let effects =
            workbench.handle_events(&[key(KeyCode::Char('1')), key(KeyCode::Enter)], area);

        assert!(effects.is_empty());
        let command = workbench
            .take_repository_command()
            .expect("fetch command queued");
        assert_eq!(command.action, RepositoryAction::Fetch);
        assert_eq!(
            workbench.diff.model.network_operation(),
            Some(NetworkOperation::Fetch)
        );
    }
}

#[test]
fn command_progress_survives_activity_switching_and_animates_the_app_border() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    workbench.commands.enqueue(RepositoryAction::Fetch);
    let _running = workbench
        .take_repository_command()
        .expect("fetch command starts");
    let area = Rect::new(0, 0, 100, 30);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|frame| workbench.render(frame)).unwrap();
    let first_border = terminal.backend().buffer()[(0, 0)].fg;
    let _ = workbench.handle_event(&key(KeyCode::Tab), area);
    for _ in 0..4 {
        workbench.tick();
    }
    terminal.draw(|frame| workbench.render(frame)).unwrap();

    let screen = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert_eq!(workbench.active, Activity::Explorer);
    assert!(screen.contains("Fetching"));
    assert!(screen.contains(interaction::DISMISS));
    assert_ne!(terminal.backend().buffer()[(0, 0)].fg, first_border);
}

#[test]
fn clicking_the_progress_marker_requests_cancellation_until_acknowledged() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    workbench.commands.enqueue(RepositoryAction::Fetch);
    let running = workbench
        .take_repository_command()
        .expect("fetch command starts");
    let area = Rect::new(0, 0, 100, 30);
    let content = workbench_areas(area).content;
    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: content.right().saturating_sub(3),
        row: content.y.saturating_add(1),
        modifiers: KeyModifiers::NONE,
    });

    let _ = workbench.handle_event(&click, area);

    assert!(running.cancellation.is_cancelled());
    assert_eq!(
        workbench.commands.active().map(|command| command.state),
        Some(CommandState::Cancelling)
    );
    workbench.operation_cancelled(running.id, RepositoryAction::Fetch);
    assert!(workbench.commands.active().is_none());
    assert!(workbench.toasts.as_slice().is_empty());
}

#[test]
fn operation_toasts_render_in_diff_and_explorer() {
    for activity in [Activity::Diff, Activity::Explorer] {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.active = activity;
        assert_eq!(
            workbench
                .diff
                .model
                .start_repository_action(RepositoryAction::Pull),
            Some(RepositoryAction::Pull)
        );
        let id = workbench.commands.enqueue(RepositoryAction::Pull);
        let _ = workbench.commands.start_next();
        workbench.operation_completed(
            id,
            RepositoryAction::Pull,
            OperationResult::Pull { commits: 1 },
            RepositorySnapshot::default(),
        );
        assert_eq!(workbench.diff.model.network_operation(), None);
        assert_eq!(workbench.toasts.as_slice()[0].title, "Pulled 1 commit");
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| workbench.render(frame)).unwrap();

        let screen = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(
            screen.contains("Pulled 1 commit"),
            "missing operation toast in {activity:?}"
        );
    }
}

#[test]
fn explorer_can_click_dismiss_a_workbench_toast() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    workbench.active = Activity::Explorer;
    workbench.show_toast(ToastKind::Info, "Fetch complete");
    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 70,
        row: 26,
        modifiers: KeyModifiers::NONE,
    });

    let _ = workbench.handle_event(&click, Rect::new(0, 0, 100, 30));

    assert!(workbench.toasts.as_slice().is_empty());
}

#[test]
fn network_activity_does_not_own_the_toast_queue() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    workbench.show_toast(ToastKind::Info, "Existing result");
    let existing_id = workbench.toasts.as_slice()[0].id;

    assert_eq!(
        workbench
            .diff
            .model
            .start_repository_action(RepositoryAction::Fetch),
        Some(RepositoryAction::Fetch)
    );
    let id = workbench.commands.enqueue(RepositoryAction::Fetch);
    let _ = workbench.commands.start_next();
    assert_eq!(
        workbench.diff.model.network_operation(),
        Some(NetworkOperation::Fetch)
    );
    assert!(
        workbench
            .toasts
            .as_slice()
            .iter()
            .any(|toast| toast.id == existing_id)
    );

    workbench.operation_completed(
        id,
        RepositoryAction::Fetch,
        OperationResult::Fetch { updated_refs: 0 },
        RepositorySnapshot::default(),
    );

    assert_eq!(workbench.diff.model.network_operation(), None);
    assert!(
        workbench
            .toasts
            .as_slice()
            .iter()
            .any(|toast| toast.id == existing_id)
    );
}

#[test]
fn command_palette_shortcut_does_not_capture_commit_message_input() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let _ = update(&mut workbench.diff.model, Message::FocusCommitInput);

    let effects = workbench.handle_events(&[key(KeyCode::Char('1'))], Rect::default());

    assert!(effects.is_empty());
    assert_eq!(workbench.diff.model.commit_message, "1");
    assert!(!workbench.active_palette().is_open());
}

#[test]
fn explorer_palette_combines_shared_and_explorer_commands() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    workbench.active = Activity::Explorer;
    workbench.open_active_palette();
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|frame| workbench.render(frame)).unwrap();

    let screen = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(screen.contains("Git: Fetch"));
    assert!(screen.contains("Explorer: Collapse All Folders"));
}

#[test]
fn prompt_modal_has_priority_and_keeps_network_operation_pending() {
    let area = Rect::new(0, 0, 100, 30);
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let command_id = start_repository_command(&mut workbench, RepositoryAction::Fetch);
    assert!(workbench.open_prompt(
        command_id,
        PromptId(1),
        GitPrompt::Username {
            host: "example.com".to_owned()
        }
    ));

    let effects = workbench.handle_events(
        &[
            key(KeyCode::Tab),
            key(KeyCode::Char('u')),
            key(KeyCode::Char('s')),
            key(KeyCode::Char('e')),
            key(KeyCode::Char('r')),
            key(KeyCode::Enter),
        ],
        area,
    );

    assert_eq!(workbench.active, Activity::Diff);
    assert_eq!(
        effects,
        vec![WorkbenchEffect::Prompt {
            command_id,
            prompt_id: PromptId(1),
            response: PromptResponse::Text("user".to_owned())
        }]
    );
    assert_eq!(
        workbench.diff.model.network_operation(),
        Some(NetworkOperation::Fetch)
    );
}

#[test]
fn secret_input_is_masked_in_frames_and_debug_output() {
    let area = Rect::new(0, 0, 80, 24);
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let command_id = start_repository_command(&mut workbench, RepositoryAction::Fetch);
    assert!(workbench.open_prompt(
        command_id,
        PromptId(1),
        GitPrompt::Secret {
            kind: diffo_core::SecretKind::HttpsSecret,
            context: "example.com".to_owned(),
        }
    ));
    let sentinel = "sentinel-secret";
    let events = sentinel
        .chars()
        .map(|character| key(KeyCode::Char(character)))
        .collect::<Vec<_>>();
    assert!(workbench.handle_events(&events, area).is_empty());
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal.draw(|frame| workbench.render(frame)).unwrap();

    let screen = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(!screen.contains(sentinel));
    assert!(screen.contains("••••"));
    let effects = workbench.handle_events(&[key(KeyCode::Enter)], area);
    assert!(!format!("{effects:?}").contains(sentinel));
    assert!(matches!(
        effects.as_slice(),
        [WorkbenchEffect::Prompt {
            response: PromptResponse::Text(answer),
            ..
        }] if answer == sentinel
    ));
}

#[test]
fn ssh_confirmation_is_cancel_first_and_supports_picker_controls() {
    let area = Rect::new(0, 0, 100, 30);
    let prompt = || GitPrompt::ConfirmSshHost {
        host: "git.example.com".to_owned(),
        fingerprint: "SHA256:abc".to_owned(),
    };
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let command_id = start_repository_command(&mut workbench, RepositoryAction::Fetch);
    assert!(workbench.open_prompt(command_id, PromptId(1), prompt()));
    assert_eq!(
        workbench.prompt.as_ref().map(|modal| modal.confirm_choice),
        Some(ConfirmChoice::Cancel)
    );
    assert_eq!(
        workbench.handle_events(&[key(KeyCode::Enter)], area),
        vec![WorkbenchEffect::Prompt {
            command_id,
            prompt_id: PromptId(1),
            response: PromptResponse::Cancel,
        }]
    );

    assert!(workbench.open_prompt(command_id, PromptId(2), prompt()));
    assert_eq!(
        workbench.handle_events(&[key(KeyCode::Right), key(KeyCode::Enter)], area),
        vec![WorkbenchEffect::Prompt {
            command_id,
            prompt_id: PromptId(2),
            response: PromptResponse::Confirm,
        }]
    );

    assert!(workbench.open_prompt(command_id, PromptId(3), prompt()));
    let button = prompt_layout(area).continue_button;
    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: button.x,
        row: button.y,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(
        workbench.handle_events(&[click], area),
        vec![WorkbenchEffect::Prompt {
            command_id,
            prompt_id: PromptId(3),
            response: PromptResponse::Confirm,
        }]
    );
}

#[test]
fn prompt_rejects_concurrent_stale_ids_and_escape_cancels() {
    let area = Rect::new(0, 0, 100, 30);
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let command_id = start_repository_command(&mut workbench, RepositoryAction::Fetch);
    let prompt = GitPrompt::Username {
        host: "example.com".to_owned(),
    };
    assert!(workbench.open_prompt(command_id, PromptId(1), prompt.clone()));
    assert!(!workbench.open_prompt(command_id, PromptId(2), prompt.clone()));
    assert_eq!(
        workbench.handle_events(&[key(KeyCode::Esc)], area),
        vec![WorkbenchEffect::Prompt {
            command_id,
            prompt_id: PromptId(1),
            response: PromptResponse::Cancel,
        }]
    );
    assert!(!workbench.open_prompt(command_id, PromptId(1), prompt));
    assert_eq!(
        workbench.commands.active().map(|command| command.state),
        Some(CommandState::Cancelling)
    );
}

#[test]
fn prompt_ids_are_scoped_to_the_active_command() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let first = start_repository_command(&mut workbench, RepositoryAction::Fetch);
    workbench.commands.enqueue(RepositoryAction::Pull);
    assert!(workbench.open_prompt(
        first,
        PromptId(1),
        GitPrompt::Username {
            host: "example.com".to_owned(),
        },
    ));
    assert!(workbench.take_repository_command().is_none());
    let _ = workbench.handle_events(
        &[key(KeyCode::Char('u')), key(KeyCode::Enter)],
        Rect::default(),
    );
    workbench.operation_completed(
        first,
        RepositoryAction::Fetch,
        OperationResult::Fetch { updated_refs: 0 },
        RepositorySnapshot::default(),
    );

    let second = workbench
        .take_repository_command()
        .expect("queued pull starts after fetch completion")
        .id;
    assert!(!workbench.open_prompt(
        first,
        PromptId(2),
        GitPrompt::Username {
            host: "stale.example.com".to_owned(),
        },
    ));
    assert!(workbench.open_prompt(
        second,
        PromptId(1),
        GitPrompt::Username {
            host: "example.com".to_owned(),
        },
    ));
}
