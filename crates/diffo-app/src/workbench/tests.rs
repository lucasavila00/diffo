use super::*;
use crate::diff::NetworkOperation;
use crossterm::event::{KeyEvent, KeyEventState, MouseEvent};
use diffo_core::{
    ChangeKind, FileDiff, FileState, OperationResult, RepositoryUpdate, RepositoryUpdateKind,
    SyncPlan,
};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};

mod momentum;
mod prompt;
mod sync;

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn buffer_region(buffer: &Buffer, area: Rect) -> Buffer {
    let mut region = Buffer::empty(Rect::new(0, 0, area.width, area.height));
    for y in 0..area.height {
        for x in 0..area.width {
            region[(x, y)] = buffer[(area.x + x, area.y + y)].clone();
        }
    }
    region
}

fn start_repository_command(
    workbench: &mut Workbench,
    action: RepositoryAction,
) -> ApplicationCommandId {
    let id = workbench.commands.enqueue(action);
    assert_eq!(
        workbench
            .take_application_command(Instant::now())
            .map(|command| command.id),
        Some(id)
    );
    id
}

#[test]
fn redraw_requests_track_visible_transitions_instead_of_idle_iterations() {
    let area = Rect::new(0, 0, 100, 30);
    let mut workbench = Workbench::new(RepositorySnapshot::default());

    assert!(workbench.take_redraw_request());
    assert!(!workbench.take_redraw_request());

    workbench.prepare_frame(area);
    assert!(workbench.take_redraw_request());
    workbench.prepare_frame(area);
    workbench.tick(Instant::now());
    assert!(!workbench.take_redraw_request());

    assert!(workbench.accept_repository_update(RepositoryUpdate {
        generation: 1,
        kind: RepositoryUpdateKind::Snapshot(RepositorySnapshot::default()),
    }));
    assert!(!workbench.take_redraw_request());
    workbench.accept_task_result(WorkbenchTaskResult::Explorer(ExplorerOutcome::Paths {
        id: u64::MAX,
        result: Ok(vec!["stale.txt".into()]),
    }));
    assert!(!workbench.take_redraw_request());

    let uppercase = Event::Key(KeyEvent::new(KeyCode::Char('O'), KeyModifiers::SHIFT));
    let _ = workbench.handle_events(&[uppercase], area);
    assert!(!workbench.take_redraw_request());

    let _ = workbench.handle_events(&[key(KeyCode::Tab)], area);
    assert!(workbench.take_redraw_request());
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
    assert_eq!(workbench.active, Activity::Diff);
    assert_eq!(workbench.diff.model.diff_scroll, 17);
}

#[test]
fn every_activity_renders_the_same_repository_footer() {
    let snapshot = RepositorySnapshot {
        head: diffo_core::HeadState::Named {
            name: "main".to_owned(),
            commit: "123456789abcdef".to_owned(),
        },
        ..RepositorySnapshot::default()
    };
    let area = Rect::new(0, 0, 100, 30);
    let status = tool_areas(workbench_areas(area).content).status;
    let mut footers = Vec::new();

    for activity in [Activity::Diff, Activity::Explorer] {
        let mut workbench = Workbench::new(snapshot.clone());
        workbench.active = activity;
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| workbench.render(frame)).unwrap();
        footers.push(buffer_region(terminal.backend().buffer(), status));
    }

    assert_eq!(footers[0], footers[1]);
    let text = footers[0]
        .content
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(text.starts_with(" main · 1234567 · clean"), "{text}");
    assert!(
        text.ends_with("[ Commands (1 / F1) ] [ Help (2 / F2) ] [ Sync (9 / F9) ]"),
        "{text}"
    );
}

#[test]
fn diff_m_opens_the_commit_modal_and_normal_enter_commits() {
    let snapshot = RepositorySnapshot {
        files: vec![FileState {
            path: "src/main.rs".into(),
            old_path: None,
            kind: ChangeKind::Modified,
            staged: Some(FileDiff {
                text: "@@ -1 +1 @@\n-old\n+new\n".to_owned(),
            }),
            unstaged: None,
        }],
        ..RepositorySnapshot::default()
    };
    let area = Rect::new(0, 0, 100, 30);
    let mut workbench = Workbench::new(snapshot);
    workbench.prepare_frame(area);

    let _ = workbench.handle_events(&[key(KeyCode::Char('m'))], area);
    assert!(matches!(workbench.modal, Some(Modal::CommitEditor)));
    let _ = workbench.handle_events(&[key(KeyCode::Esc)], area);
    assert!(workbench.modal.is_none());

    let _ = workbench.handle_events(&[key(KeyCode::Enter)], area);

    assert!(workbench.modal.is_none());
    assert_eq!(workbench.commands.queued_len(), 1);
    assert!(!workbench.diff.model.commit_enabled());
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
    assert_eq!(workbench.active, Activity::Diff);
}

#[test]
fn full_screen_diff_renders_styled_raw_hunks_and_x_closes_it() {
    let snapshot = RepositorySnapshot {
        files: vec![FileState {
            path: "src/main.rs".into(),
            old_path: None,
            kind: ChangeKind::Modified,
            staged: None,
            unstaged: Some(FileDiff {
                text: "@@ -1 +1 @@\n-let old = true;\n+let new = false;\n".to_owned(),
            }),
        }],
        ..RepositorySnapshot::default()
    };
    let area = Rect::new(0, 0, 80, 8);
    let mut workbench = Workbench::new(snapshot);
    workbench.prepare_frame(area);

    let entry = full_screen::entry_area(area, workbench.pane_split);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| workbench.render(frame)).unwrap();
    assert_eq!(
        terminal.backend().buffer()[(entry.x, entry.y)].symbol(),
        diffo_ui::icons::MAXIMIZE,
    );
    let normal_header = (0..area.width)
        .map(|column| terminal.backend().buffer()[(column, area.y)].symbol())
        .collect::<String>();
    assert!(normal_header.contains("Inline ─── "), "{normal_header}");

    let open = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: entry.x,
        row: entry.y,
        modifiers: KeyModifiers::NONE,
    });
    let _ = workbench.handle_event(&open, area);
    workbench.prepare_frame(area);
    assert!(workbench.full_screen());

    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| workbench.render(frame)).unwrap();
    let row = |row| {
        (0..area.width)
            .map(|column| terminal.backend().buffer()[(column, row)].symbol())
            .collect::<String>()
    };
    assert!(row(0).starts_with("M src/main.rs"));
    assert_eq!(terminal.backend().buffer()[(0, 0)].fg, Color::Yellow);
    assert_eq!(
        terminal.backend().buffer()[(area.right().saturating_sub(1), 0)].symbol(),
        diffo_ui::icons::DISMISS,
    );
    assert!(row(1).starts_with("@@ -1 +1 @@"));
    assert!(row(2).starts_with("-let old = true;"));
    assert!(row(3).starts_with("+let new = false;"));
    assert_eq!(terminal.backend().buffer()[(0, 2)].bg, Color::Indexed(52));
    assert_eq!(terminal.backend().buffer()[(0, 3)].bg, Color::Indexed(22));
    assert!(!row(0).contains("File Diff"));

    let _ = workbench.handle_event(&key(KeyCode::Char('F')), area);
    assert!(workbench.full_screen());
    let close = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: area.right().saturating_sub(1),
        row: 0,
        modifiers: KeyModifiers::NONE,
    });
    let _ = workbench.handle_event(&close, area);
    assert!(!workbench.full_screen());
}

#[test]
fn commit_input_keeps_f_as_text_instead_of_opening_full_screen() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    workbench.set_modal(Modal::CommitEditor);

    assert!(
        workbench
            .handle_events(&[key(KeyCode::Char('f'))], Rect::new(0, 0, 80, 24))
            .is_empty()
    );

    assert_eq!(workbench.diff.model.commit_message, "f");
    assert!(!workbench.full_screen());
    assert!(!workbench.full_screen_pending);
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
    assert_eq!(workbench.active, Activity::Diff);
    assert_eq!(workbench.pane_split.areas(pane_area).trailing.x, 62);
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

    for code in [KeyCode::Char('f'), KeyCode::Char('1'), KeyCode::Char('q')] {
        let _ = workbench.handle_event(&key(code), area);
    }
    assert_eq!(workbench.pane_split.percent(), 25);
    assert!(workbench.modal.is_none());
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
fn shared_git_commands_execute_from_every_activity() {
    let area = Rect::new(0, 0, 100, 30);
    for activity in [Activity::Diff, Activity::Explorer] {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.active = activity;

        let effects =
            workbench.handle_events(&[key(KeyCode::Char('1')), key(KeyCode::Enter)], area);

        assert!(effects.is_empty());
        let command = workbench
            .take_application_command(Instant::now())
            .expect("fetch command queued");
        assert_eq!(
            command.action,
            ApplicationAction::Repository(RepositoryAction::Fetch)
        );
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
    let started = Instant::now();
    let _running = workbench
        .take_application_command(started)
        .expect("fetch command starts");
    let area = Rect::new(0, 0, 100, 30);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();

    workbench.tick(started + Duration::from_millis(150));
    terminal.draw(|frame| workbench.render(frame)).unwrap();
    let first_border = terminal.backend().buffer()[(0, 0)].fg;
    let _ = workbench.handle_event(&key(KeyCode::Tab), area);
    for _ in 0..4 {
        workbench.tick(started + Duration::from_millis(150));
    }
    terminal.draw(|frame| workbench.render(frame)).unwrap();

    assert_eq!(workbench.active, Activity::Explorer);
    insta::assert_debug_snapshot!(buffer_region(
        terminal.backend().buffer(),
        Rect::new(55, 1, 44, 3),
    ));
    assert_ne!(terminal.backend().buffer()[(0, 0)].fg, first_border);
}

#[test]
fn clicking_the_progress_marker_requests_cancellation_until_acknowledged() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    workbench.commands.enqueue(RepositoryAction::Fetch);
    let started = Instant::now();
    let running = workbench
        .take_application_command(started)
        .expect("fetch command starts");
    workbench.tick(started + Duration::from_millis(150));
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
    workbench.operation_cancelled(
        running.id,
        RepositoryAction::Fetch,
        RepositorySnapshot::default(),
    );
    assert!(workbench.commands.active().is_none());
    assert!(workbench.toasts.as_slice().is_empty());
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
fn explorer_palette_combines_shared_and_explorer_commands() {
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    workbench.active = Activity::Explorer;
    workbench.open_active_palette();
    let Some(Modal::CommandPalette(palette)) = workbench.modal.as_ref() else {
        panic!("command palette modal should be open");
    };
    let commands = palette
        .matches()
        .into_iter()
        .map(|command| format!("{:?}: {}", command.id, command.label))
        .collect::<Vec<_>>();

    insta::assert_debug_snapshot!(commands);
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
        match workbench.modal.as_ref() {
            Some(Modal::GitPrompt(modal)) => Some(modal.confirm_choice),
            _ => None,
        },
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
    let button = prompt_layout(area, true).continue_button;
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
