use super::*;

#[test]
fn protected_branch_push_is_cancel_first_and_captures_all_input() {
    let area = Rect::new(0, 0, 100, 30);
    let prompt = || GitPrompt::ConfirmProtectedBranchPush {
        destination: "origin/main".to_owned(),
        commits: 2,
    };
    let mut workbench = Workbench::new(RepositorySnapshot::default());
    let command_id = start_repository_command(&mut workbench, RepositoryAction::Sync);
    assert!(workbench.open_prompt(command_id, PromptId(1), prompt()));
    assert_eq!(
        match workbench.modal.as_ref() {
            Some(Modal::GitPrompt(modal)) => Some(modal.confirm_choice),
            _ => None,
        },
        Some(ConfirmChoice::Cancel)
    );

    assert!(
        workbench
            .handle_events(&[key(KeyCode::Char('9'))], area)
            .is_empty()
    );
    assert!(matches!(workbench.modal, Some(Modal::GitPrompt(_))));

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
    assert!(screen.contains("Push 2 commits directly to origin/main?"));
    assert!(screen.contains("This bypasses the branch and pull-request workflow."));
    assert!(screen.contains("[ Push ]"));

    assert_eq!(
        workbench.handle_events(&[key(KeyCode::Right), key(KeyCode::Enter)], area),
        vec![WorkbenchEffect::Prompt {
            command_id,
            prompt_id: PromptId(1),
            response: PromptResponse::Confirm,
        }]
    );

    assert!(workbench.open_prompt(command_id, PromptId(2), prompt()));
    let modal = prompt_layout(area, true).modal;
    let outside_click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: modal.x.saturating_sub(1),
        row: modal.y,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(
        workbench.handle_events(&[outside_click], area),
        vec![WorkbenchEffect::Prompt {
            command_id,
            prompt_id: PromptId(2),
            response: PromptResponse::Cancel,
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
    workbench.commands.enqueue(RepositoryAction::Sync);
    assert!(workbench.open_prompt(
        first,
        PromptId(1),
        GitPrompt::Username {
            host: "example.com".to_owned(),
        },
    ));
    assert!(workbench.take_application_command(Instant::now()).is_none());
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
        .take_application_command(Instant::now())
        .expect("queued sync starts after fetch completion")
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
