use super::*;

#[test]
fn change_navigation_backgrounds_follow_target_content() {
    let inline_model = model();
    let area = Rect::new(0, 0, 100, 30);
    let mut inline_renderer = Renderer::new();
    inline_renderer.prepare_frame(&inline_model, area);
    let inline = &inline_renderer.highlighted.as_ref().unwrap().inline;
    let removed = inline
        .iter()
        .position(|row| row.kind == RowKind::Removed)
        .unwrap();
    let added = inline
        .iter()
        .position(|row| row.kind == RowKind::Added)
        .unwrap();

    assert_eq!(
        inline_renderer.change_navigation_background(removed, true),
        diff_background(RowKind::Removed)
    );
    assert_eq!(
        inline_renderer.change_navigation_background(added, false),
        diff_background(RowKind::Added)
    );

    let mut side_model = model();
    side_model.diff_view_mode = DiffViewMode::SideBySide;
    let mut side_renderer = Renderer::new();
    side_renderer.prepare_frame(&side_model, area);
    let replacement = side_renderer
        .highlighted
        .as_ref()
        .unwrap()
        .side_by_side
        .iter()
        .position(|row| row.kind == RowKind::Changed)
        .unwrap();

    assert_eq!(
        side_renderer.change_navigation_background(replacement, false),
        diff_background(RowKind::Removed)
    );
    assert_eq!(
        side_renderer.change_navigation_background(replacement, true),
        diff_background(RowKind::Added)
    );

    let mut conflict_model = model();
    conflict_model.snapshot.files[0].kind = ChangeKind::Conflicted;
    conflict_model.snapshot.files[0]
        .unstaged
        .as_mut()
        .unwrap()
        .text = "@@ -1 +1,3 @@\n-old\n+<<<<<<< HEAD\n+ours\n+>>>>>>> branch\n".to_owned();
    let mut conflict_renderer = Renderer::new();
    conflict_renderer.prepare_frame(&conflict_model, area);
    let conflict = conflict_renderer
        .highlighted
        .as_ref()
        .unwrap()
        .inline
        .iter()
        .position(|row| row.kind == RowKind::Conflict)
        .unwrap();

    assert_eq!(
        conflict_renderer.change_navigation_background(conflict, true),
        diff_background(RowKind::Conflict)
    );
}

#[test]
fn command_progress_animates_and_exposes_only_the_cancel_marker() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let rows = vec![crate::diff::CommandProgressRow {
        id: diffo_core::ApplicationCommandId(1),
        label: "Pushing".to_owned(),
        state: crate::diff::CommandProgressState::Active,
    }];
    terminal
        .draw(|frame| {
            crate::diff::render_command_progress(
                frame,
                crate::diff::CommandProgress {
                    rows: &rows,
                    hidden: 0,
                    animation_tick: 0,
                },
                frame.area(),
            );
        })
        .unwrap();
    insta::assert_debug_snapshot!(buffer_region(
        terminal.backend().buffer(),
        Rect::new(35, 1, 44, 6),
    ));
    let progress = crate::diff::CommandProgress {
        rows: &rows,
        hidden: 0,
        animation_tick: 0,
    };
    assert_eq!(
        crate::diff::command_at_position(progress, Rect::new(0, 0, 80, 24), 77, 2),
        Some(diffo_core::ApplicationCommandId(1))
    );
}

#[test]
fn command_progress_shows_order_overflow_and_every_cancel_target() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let rows = vec![
        crate::diff::CommandProgressRow {
            id: diffo_core::ApplicationCommandId(1),
            label: "Sync — Fetching".to_owned(),
            state: crate::diff::CommandProgressState::Active,
        },
        crate::diff::CommandProgressRow {
            id: diffo_core::ApplicationCommandId(2),
            label: "AI commit".to_owned(),
            state: crate::diff::CommandProgressState::Queued,
        },
        crate::diff::CommandProgressRow {
            id: diffo_core::ApplicationCommandId(3),
            label: "Sync".to_owned(),
            state: crate::diff::CommandProgressState::Queued,
        },
    ];
    terminal
        .draw(|frame| {
            crate::diff::render_command_progress(
                frame,
                crate::diff::CommandProgress {
                    rows: &rows,
                    hidden: 2,
                    animation_tick: 0,
                },
                frame.area(),
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let line = |row| {
        (35..79)
            .map(|column| buffer[(column, row)].symbol())
            .collect::<Vec<_>>()
            .concat()
    };
    assert!(line(2).contains("Sync — Fetching"));
    assert!(line(1).contains("Commands (+2 more)"));
    assert!(line(3).contains("1. AI commit"));
    assert!(line(4).contains("2. Sync"));

    let progress = crate::diff::CommandProgress {
        rows: &rows,
        hidden: 2,
        animation_tick: 0,
    };
    assert_eq!(
        crate::diff::command_at_position(progress, Rect::new(0, 0, 80, 24), 77, 3),
        Some(diffo_core::ApplicationCommandId(2))
    );
    assert_eq!(
        crate::diff::command_at_position(progress, Rect::new(0, 0, 80, 24), 77, 5),
        None
    );
}

#[test]
fn command_progress_does_not_draw_clipped_controls_on_tiny_areas() {
    let rows = vec![crate::diff::CommandProgressRow {
        id: diffo_core::ApplicationCommandId(1),
        label: "Sync — Fetching".to_owned(),
        state: crate::diff::CommandProgressState::Active,
    }];
    for area in [Rect::new(0, 0, 20, 24), Rect::new(0, 0, 80, 4)] {
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                crate::diff::render_command_progress(
                    frame,
                    crate::diff::CommandProgress {
                        rows: &rows,
                        hidden: 0,
                        animation_tick: 0,
                    },
                    area,
                );
            })
            .unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(!rendered.contains("Commands"));
        assert_eq!(
            crate::diff::command_at_position(
                crate::diff::CommandProgress {
                    rows: &rows,
                    hidden: 0,
                    animation_tick: 0,
                },
                area,
                area.right().saturating_sub(2),
                2,
            ),
            None
        );
    }
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
fn disabled_commit_composer_action_uses_the_disabled_control_style() {
    let model = model();
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| crate::diff::render_commit_composer(frame, frame.area(), &model))
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer());
}

#[test]
fn fixed_commit_composer_action_uses_the_enabled_control_style() {
    let mut model = model();
    model.snapshot.files[0].staged = Some(FileDiff {
        text: String::new(),
    });
    let backend = TestBackend::new(40, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| crate::diff::render_commit_composer(frame, frame.area(), &model))
        .unwrap();

    insta::assert_debug_snapshot!(terminal.backend().buffer());
}

#[test]
fn ready_merge_uses_merge_labels_in_both_commit_controls() {
    let mut model = model();
    model.snapshot.operation = RepositoryOperationState::Merge;
    model.snapshot.files.clear();
    let area = Rect::new(0, 0, 80, 24);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| crate::diff::render_commit_composer(frame, frame.area(), &model))
        .unwrap();
    let text = buffer_text(terminal.backend().buffer());
    assert!(text.contains("Complete merge"));
    assert!(!text.contains("Commit (Enter)"));

    terminal
        .draw(|frame| crate::diff::render_commit_editor(frame, &model, frame.area()))
        .unwrap();
    let text = buffer_text(terminal.backend().buffer());
    assert!(text.contains("Complete merge (Enter)"));
}

#[test]
fn commit_dialog_actions_use_the_enabled_control_style() {
    let mut model = model();
    model.snapshot.files[0].staged = Some(FileDiff {
        text: String::new(),
    });
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
