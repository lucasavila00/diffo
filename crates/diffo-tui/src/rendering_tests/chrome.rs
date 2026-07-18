use super::*;

#[test]
fn command_progress_animates_and_exposes_only_the_cancel_marker() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            crate::render_command_progress(
                frame,
                crate::CommandProgress {
                    label: "Pushing",
                    cancelling: false,
                    animation_tick: 0,
                },
                frame.area(),
            );
        })
        .unwrap();
    let screen =
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .fold(String::new(), |mut output, cell| {
                output.push_str(cell.symbol());
                output
            });
    assert!(screen.contains("Pushing"));
    assert!(screen.contains(interaction::DISMISS));
    assert!(crate::command_cancel_at_position(
        Rect::new(0, 0, 80, 24),
        77,
        1
    ));
    assert!(!crate::command_cancel_at_position(
        Rect::new(0, 0, 80, 24),
        76,
        1
    ));
}

#[test]
fn commit_message_and_file_diff_boxes_share_the_chrome_border() {
    let model = model();
    let area = Rect::new(0, 0, 80, 24);
    let panes = horizontal_panes(main_area(area), model.file_pane_percent);
    let mut renderer = Renderer::new();
    renderer.prepare_frame(&model, area);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    let buffer = terminal.backend().buffer();
    let commit_border = buffer[(panes[0].x, panes[0].y)].fg;
    let diff_border = buffer[(panes[1].x, panes[1].y)].fg;
    assert_eq!(commit_border, theme::CHROME);
    assert_eq!(diff_border, theme::CHROME);
}

#[test]
fn commit_composer_panel_action_uses_the_enabled_control_style() {
    let model = model();
    let backend = TestBackend::new(20, 6);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| crate::render_commit_composer(frame, frame.area(), &model))
        .unwrap();

    let edit_control = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .find(|cell| cell.symbol() == interaction::EDIT)
        .expect("click-to-edit control");
    assert_eq!(edit_control.fg, theme::TEXT);
    assert!(edit_control.modifier.contains(Modifier::BOLD));
}

#[test]
fn blocked_sync_is_visually_clickable_because_it_opens_feedback() {
    let mut model = model();
    model.snapshot.upstream = Some(UpstreamState {
        name: "origin/main".to_owned(),
        ahead: 1,
        behind: 1,
    });
    let backend = TestBackend::new(40, 6);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| crate::render_commit_composer(frame, frame.area(), &model))
        .unwrap();

    let action = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .find(|cell| cell.symbol() == "[")
        .expect("blocked sync action");
    assert_eq!(action.fg, theme::TEXT);
    assert!(action.modifier.contains(Modifier::BOLD));
}

#[test]
fn commit_dialog_actions_use_the_enabled_control_style() {
    let mut model = model();
    model.snapshot.files[0].staged = Some(FileDiff {
        text: String::new(),
    });
    model.focus_commit_input();
    model.commit_message_input('x');
    let area = Rect::new(0, 0, 80, 24);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| crate::render_commit_editor(frame, &model, frame.area()))
        .unwrap();

    let actions = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .filter(|cell| cell.symbol() == "[")
        .collect::<Vec<_>>();
    assert_eq!(actions.len(), 2);
    for action in actions {
        assert_eq!(action.fg, theme::TEXT);
        assert!(action.modifier.contains(Modifier::BOLD));
    }
    let footer = crate::overlays::commit_editor_layout(area).4;
    let footer_control = (footer.x..footer.right())
        .map(|column| &terminal.backend().buffer()[(column, footer.y)])
        .find(|cell| cell.symbol() != " ")
        .expect("outside-click instruction");
    assert_eq!(footer_control.fg, theme::TEXT);
    assert!(footer_control.modifier.contains(Modifier::BOLD));
}

#[test]
fn renders_and_hit_tests_a_bottom_right_toast() {
    let mut toasts = ToastQueue::new();
    toasts.show(ToastKind::Success, "Committed a1b2c3d");
    let id = toasts.as_slice()[0].id;
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| crate::render_toasts(frame, toasts.as_slice(), frame.area()))
        .unwrap();
    assert!(
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .any(|cell| { cell.symbol().contains("Committed") || cell.fg == Color::LightGreen })
    );
    let dismiss = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .find(|cell| cell.symbol() == interaction::DISMISS)
        .expect("toast dismiss control");
    assert_eq!(dismiss.fg, theme::TEXT);
    assert!(dismiss.modifier.contains(Modifier::BOLD));

    assert_eq!(
        crate::toast_at_position(toasts.as_slice(), Rect::new(0, 0, 100, 30), 70, 26),
        Some(id)
    );
}

#[test]
fn error_toasts_render_embedded_newlines_as_inert_text() {
    let toasts = [Toast {
        id: 1,
        kind: ToastKind::Error,
        title: "Push failed\naccept remote output?".to_owned(),
        detail: Some("detail\nnext line".to_owned()),
    }];
    let area = Rect::new(0, 0, 100, 30);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| crate::render_toasts(frame, &toasts, frame.area()))
        .unwrap();

    let screen = buffer_text(terminal.backend().buffer());
    assert!(screen.contains("Push failed␊accept remote output?"));
    assert!(screen.contains("detail␊next line"));
    assert!(!screen.chars().any(char::is_control));
    assert_eq!(crate::toast_at_position(&toasts, area, 70, 26), Some(1));
}
