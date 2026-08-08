use super::*;

fn buffer_text_position(buffer: &Buffer, area: Rect, text: &str) -> Option<(u16, u16)> {
    let width = u16::try_from(text.chars().count()).ok()?;
    for row in area.y..area.bottom() {
        for column in area.x..area.right().saturating_sub(width).saturating_add(1) {
            let visible = (0..width)
                .map(|offset| buffer[(column.saturating_add(offset), row)].symbol())
                .collect::<String>();
            if visible == text {
                return Some((column, row));
            }
        }
    }
    None
}

#[test]
fn only_plain_diff_change_navigation_is_a_wheel_cancellation_intent() {
    let navigation = |character, modifiers, kind| {
        Event::Key(KeyEvent {
            code: KeyCode::Char(character),
            modifiers,
            kind,
            state: KeyEventState::NONE,
        })
    };
    let next = navigation('n', KeyModifiers::NONE, KeyEventKind::Press);
    let previous = navigation('p', KeyModifiers::NONE, KeyEventKind::Press);
    let mut workbench = Workbench::new(RepositorySnapshot::default());

    assert!(workbench.is_diff_change_navigation(&next));
    assert!(workbench.is_diff_change_navigation(&previous));
    for event in [
        navigation('N', KeyModifiers::SHIFT, KeyEventKind::Press),
        navigation('P', KeyModifiers::SHIFT, KeyEventKind::Press),
        navigation('n', KeyModifiers::CONTROL, KeyEventKind::Press),
        navigation('n', KeyModifiers::NONE, KeyEventKind::Repeat),
        navigation('p', KeyModifiers::NONE, KeyEventKind::Release),
    ] {
        assert!(!workbench.is_diff_change_navigation(&event));
    }

    workbench.active = Activity::Explorer;
    assert!(!workbench.is_diff_change_navigation(&next));
    workbench.active = Activity::Diff;
    workbench.full_screen = true;
    assert!(!workbench.is_diff_change_navigation(&next));
    workbench.full_screen = false;

    workbench.set_modal(Modal::CommitEditor);
    assert!(!workbench.is_diff_change_navigation(&next));
    workbench.set_modal(Modal::command_palette(Vec::new()));
    assert!(!workbench.is_diff_change_navigation(&next));
    workbench.close_modal();

    let command_id = start_repository_command(&mut workbench, RepositoryAction::Fetch);
    assert!(workbench.open_prompt(
        command_id,
        PromptId(1),
        GitPrompt::Username {
            host: "example.com".to_owned(),
        },
    ));
    assert!(!workbench.is_diff_change_navigation(&next));
}

#[test]
fn open_diff_picker_menu_is_not_change_navigation_context() {
    let snapshot = RepositorySnapshot {
        files: vec![FileState {
            path: "momentum.rs".into(),
            old_path: None,
            kind: ChangeKind::Modified,
            staged: None,
            unstaged: Some(FileDiff {
                text: "@@ -1 +1 @@\n-old\n+new\n".to_owned(),
            }),
        }],
        ..RepositorySnapshot::default()
    };
    let area = Rect::new(0, 0, 100, 30);
    let mut workbench = Workbench::new(snapshot);
    workbench.prepare_frame(area);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| workbench.render(frame)).unwrap();
    let content = workbench_areas(area).content;
    let file_pane = Rect::new(content.x, content.y, content.width / 4, content.height);
    let (column, row) = buffer_text_position(terminal.backend().buffer(), file_pane, "momentum.rs")
        .expect("changed file is visible in the file pane");
    let right_click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });

    let _ = workbench.handle_event(&right_click, area);

    assert!(workbench.diff.captures_global_input());
    assert!(!workbench.is_diff_change_navigation(&key(KeyCode::Char('n'))));
}
