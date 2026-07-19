use super::*;

#[test]
fn command_progress_animates_and_exposes_only_the_cancel_marker() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            crate::diff::render_command_progress(
                frame,
                crate::diff::CommandProgress {
                    label: "Pushing",
                    cancelling: false,
                    animation_tick: 0,
                },
                frame.area(),
            );
        })
        .unwrap();
    insta::assert_debug_snapshot!(buffer_region(
        terminal.backend().buffer(),
        Rect::new(35, 1, 44, 3),
    ));
    assert!(crate::diff::command_cancel_at_position(
        Rect::new(0, 0, 80, 24),
        77,
        1
    ));
    assert!(!crate::diff::command_cancel_at_position(
        Rect::new(0, 0, 80, 24),
        76,
        1
    ));
}

#[test]
fn commit_message_and_file_diff_boxes_share_the_chrome_border() {
    let model = model();
    let area = Rect::new(0, 0, 80, 24);
    let mut renderer = Renderer::new();
    renderer.prepare_frame(&model, area);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    insta::assert_debug_snapshot!(buffer_region(
        terminal.backend().buffer(),
        Rect::new(area.x, area.y, area.width, 1),
    ));
}

#[test]
fn commit_composer_panel_action_uses_the_enabled_control_style() {
    let model = model();
    let backend = TestBackend::new(20, 6);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| crate::diff::render_commit_composer(frame, frame.area(), &model))
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer());
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
        .draw(|frame| crate::diff::render_commit_composer(frame, frame.area(), &model))
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer());
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
        .draw(|frame| crate::diff::render_commit_editor(frame, &model, frame.area()))
        .unwrap();

    let editor_area = crate::diff::view::overlays::commit_editor_layout(area).0;
    insta::assert_debug_snapshot!(buffer_region(terminal.backend().buffer(), editor_area));
}

#[test]
fn renders_and_hit_tests_a_bottom_right_toast() {
    let mut toasts = ToastQueue::new();
    toasts.show(ToastKind::Success, "Committed a1b2c3d");
    let id = toasts.as_slice()[0].id;
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| crate::diff::render_toasts(frame, toasts.as_slice(), frame.area()))
        .unwrap();
    insta::assert_debug_snapshot!(buffer_region(
        terminal.backend().buffer(),
        Rect::new(55, 25, 44, 3),
    ));

    assert_eq!(
        crate::diff::toast_at_position(toasts.as_slice(), Rect::new(0, 0, 100, 30), 70, 26),
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
        .draw(|frame| crate::diff::render_toasts(frame, &toasts, frame.area()))
        .unwrap();

    let screen = buffer_text(terminal.backend().buffer());
    assert!(!screen.chars().any(char::is_control));
    insta::assert_debug_snapshot!(buffer_region(
        terminal.backend().buffer(),
        Rect::new(55, 24, 44, 4),
    ));
    assert_eq!(
        crate::diff::toast_at_position(&toasts, area, 70, 26),
        Some(1)
    );
}
