use super::*;

fn diff_lines(
    renderer: &mut Renderer,
    model: &Model,
    first_row: usize,
) -> Vec<ratatui::text::Line<'static>> {
    for _ in 0..200 {
        renderer.prepare_frame(model, Rect::new(0, 0, 100, 30));
        let lines = renderer.diff_lines(model, 80, first_row, 100);
        if !renderer.is_preparing() {
            return lines;
        }
        sleep(Duration::from_millis(1));
    }
    panic!("diff preparation timed out");
}

fn wait_for_viewport_transition(
    renderer: &mut Renderer,
    model: &Model,
) -> crate::diff::ViewportTransition {
    for _ in 0..200 {
        let preparation = renderer.prepare_frame(model, Rect::new(0, 0, 100, 30));
        if let Some(viewport) = preparation.viewport_transition {
            return viewport;
        }
        sleep(Duration::from_millis(1));
    }
    panic!("viewport preparation timed out");
}

fn wait_for_syntax_ready(renderer: &mut Renderer, model: &Model) {
    for _ in 0..200 {
        let preparation = renderer.prepare_frame(model, Rect::new(0, 0, 100, 30));
        if preparation.syntax_ready {
            return;
        }
        sleep(Duration::from_millis(1));
    }
    panic!("syntax preparation timed out");
}

#[test]
fn renders_syntax_foregrounds_over_diff_backgrounds() {
    let mut renderer = Renderer::new();
    let model = model();
    let lines = diff_lines(&mut renderer, &model, 0);
    assert!(!lines.is_empty());
    assert!(!renderer.is_preparing());
    insta::assert_debug_snapshot!(&lines[..3]);
}

#[test]
fn prepares_large_diffs_in_the_background() {
    let mut model = model();
    let mut patch = String::from("@@ -0,0 +1,501 @@\n");
    for index in 0..501 {
        writeln!(patch, "+line {index}").unwrap();
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();

    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    let pending = renderer.diff_lines(&model, 80, 0, 100);
    assert!(pending.is_empty());
    assert!(renderer.is_preparing());

    let lines = diff_lines(&mut renderer, &model, 0);
    assert!(!lines.is_empty());
    assert!(!renderer.is_preparing());
}

#[test]
fn keeps_previous_diff_visible_while_preparing() {
    let mut model = model();
    let mut renderer = Renderer::new();
    let previous = diff_lines(&mut renderer, &model, 0);
    let mut patch = String::from("@@ -0,0 +1,501 @@\n");
    for index in 0..501 {
        writeln!(patch, "+line {index}").unwrap();
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;

    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    let during_transition = renderer.diff_lines(&model, 80, 0, 100);

    assert_eq!(during_transition, previous);
    assert!(renderer.is_preparing());
}

#[test]
fn commits_a_new_file_and_its_first_change_position_together() {
    let mut model = model();
    let previous_file = model.selected.clone().unwrap();
    let mut patch = String::from("@@ -1,501 +1,501 @@\n");
    for index in 0..501 {
        if index == 449 {
            writeln!(patch, "-old line {index}").unwrap();
            writeln!(patch, "+new line {index}").unwrap();
        } else {
            writeln!(patch, " context line {index}").unwrap();
        }
    }
    model.snapshot.files.push(FileState {
        path: PathBuf::from("src/second.rs"),
        old_path: None,
        kind: ChangeKind::Modified,
        staged: None,
        unstaged: Some(FileDiff { text: patch }),
    });
    let mut renderer = Renderer::new();
    let area = Rect::new(0, 0, 100, 30);
    renderer.prepare_frame(&model, area);
    let previous = renderer.diff_lines(&model, 80, 0, 100);
    model.diff_scroll = 7;
    model.diff_horizontal_scroll = 9;
    model.select_next();

    let pending = renderer.prepare_frame(&model, area);

    assert!(pending.viewport_transition.is_none());
    assert_eq!(pending.displayed_file, Some(previous_file));
    assert_eq!(renderer.diff_lines(&model, 80, 0, 100), previous);
    assert_eq!((model.diff_scroll, model.diff_horizontal_scroll), (7, 9));

    let committed = (0..200)
        .find_map(|_| {
            let preparation = renderer.prepare_frame(&model, area);
            if preparation.viewport_transition.is_some() {
                Some(preparation)
            } else {
                sleep(Duration::from_millis(1));
                None
            }
        })
        .expect("second diff preparation timed out");
    let transition = committed.viewport_transition.unwrap();
    assert_eq!(committed.displayed_file, model.selected);
    assert_eq!(transition.vertical, 450);
    assert_eq!(transition.horizontal, 0);
    insta::assert_debug_snapshot!(
        "committed_first_change",
        renderer.diff_lines(&model, 80, transition.vertical, 1)[0]
    );
}

#[test]
fn staged_and_unstaged_buffers_of_one_path_have_distinct_identities() {
    let mut snapshot = model().snapshot;
    snapshot.files[0].staged = Some(FileDiff {
        text: "@@ -1,3 +1,3 @@\n-old\n+staged\n context\n context\n".to_owned(),
    });
    snapshot.files[0].unstaged = Some(FileDiff {
        text: "@@ -1,3 +1,3 @@\n context\n context\n-old\n+unstaged\n".to_owned(),
    });
    let mut model = Model::new(snapshot);
    model.select_previous();
    let mut renderer = Renderer::new();
    let area = Rect::new(0, 0, 100, 30);
    let staged = renderer.prepare_frame(&model, area);
    let staged_revision = staged.content_revision;
    assert_eq!(staged.viewport_transition.unwrap().vertical, 1);
    model.diff_scroll = 17;
    model.diff_horizontal_scroll = 8;

    model.select_next();
    assert_eq!((model.diff_scroll, model.diff_horizontal_scroll), (17, 8));
    let unstaged = renderer.prepare_frame(&model, area);

    assert!(unstaged.content_revision > staged_revision);
    assert_eq!(unstaged.displayed_file, model.selected);
    let transition = unstaged.viewport_transition.unwrap();
    assert_eq!(transition.vertical, 3);
    assert_eq!(transition.horizontal, 0);
}

#[test]
fn anchors_the_first_visible_row_when_content_moves_above_it() {
    let mut inline_model = model();
    let patch = |prefix: &[&str]| {
        let mut patch = format!("@@ -0,0 +1,{} @@\n", prefix.len() + 40);
        for line in prefix {
            writeln!(patch, "+{line}").unwrap();
        }
        for index in 0..40 {
            writeln!(patch, "+stable line {index}").unwrap();
        }
        patch
    };
    inline_model.snapshot.files[0]
        .unstaged
        .as_mut()
        .unwrap()
        .text = patch(&[]);
    inline_model.diff_scroll = 12;
    let mut renderer = Renderer::new();
    let area = Rect::new(0, 0, 100, 30);
    let initial = renderer.prepare_frame(&inline_model, area);
    assert_eq!(initial.viewport_transition.unwrap().vertical, 1);

    inline_model.snapshot.files[0]
        .unstaged
        .as_mut()
        .unwrap()
        .text = patch(&["inserted one", "inserted two", "inserted three"]);
    let changed = renderer.prepare_frame(&inline_model, area);

    assert_eq!(changed.viewport_transition.unwrap().vertical, 15);

    let mut side_model = model();
    side_model.diff_view_mode = DiffViewMode::SideBySide;
    side_model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch(&[]);
    side_model.diff_scroll = 12;
    let mut side_renderer = Renderer::new();
    side_renderer.prepare_frame(&side_model, area);
    side_model.snapshot.files[0].unstaged.as_mut().unwrap().text =
        patch(&["inserted one", "inserted two", "inserted three"]);

    let side_changed = side_renderer.prepare_frame(&side_model, area);

    assert_eq!(side_changed.viewport_transition.unwrap().vertical, 15);
}

#[test]
fn uses_the_next_visible_row_when_the_anchor_was_deleted() {
    let mut model = model();
    let patch = |skip: Option<usize>| {
        let mut patch = String::from("@@ -0,0 +1,40 @@\n");
        for index in 0..40 {
            if skip != Some(index) {
                writeln!(patch, "+stable line {index}").unwrap();
            }
        }
        patch
    };
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch(None);
    model.diff_scroll = 12;
    let mut renderer = Renderer::new();
    let area = Rect::new(0, 0, 100, 30);
    renderer.prepare_frame(&model, area);

    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch(Some(11));
    let changed = renderer.prepare_frame(&model, area);

    assert_eq!(changed.viewport_transition.unwrap().vertical, 11);
}

#[test]
fn renders_invalid_patches_as_raw_text() {
    let mut model = model();
    model.snapshot.files[0].unstaged.as_mut().unwrap().text =
        "diff --cc src/main.rs\n@@@ malformed\n+raw line\n".to_owned();
    let mut renderer = Renderer::new();

    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    let lines = renderer.diff_lines(&model, 80, 0, 100);

    assert!(!renderer.is_preparing());
    insta::assert_debug_snapshot!(lines);
}

#[test]
fn maps_inset_scrollbar_clicks_to_absolute_positions() {
    let mut model = model();
    let mut patch = String::from("@@ -0,0 +1,100 @@\n");
    for _ in 0..100 {
        writeln!(patch, "+{}", "x".repeat(200)).unwrap();
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    let vertical = renderer.scrollbars.vertical_area;
    let vertical_click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: vertical.right().saturating_sub(1),
        row: vertical.bottom().saturating_sub(1),
        modifiers: KeyModifiers::NONE,
    });
    let Some(RendererEvent::Message(crate::diff::Message::SetDiffScroll(vertical_target))) =
        renderer.map_event(&vertical_click, &model, Rect::new(0, 0, 100, 30))
    else {
        panic!("vertical scrollbar did not return an absolute target");
    };
    assert!(vertical_target > 0);

    renderer.scrollbar_drag = None;
    let horizontal = renderer.scrollbars.horizontal_area;
    let horizontal_click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: horizontal.right().saturating_sub(1),
        row: horizontal.bottom().saturating_sub(1),
        modifiers: KeyModifiers::NONE,
    });
    let horizontal_maximum = renderer
        .scrollbars
        .columns
        .saturating_sub(renderer.scrollbars.viewport_columns);
    assert!(matches!(
        renderer.map_event(&horizontal_click, &model, Rect::new(0, 0, 100, 30)),
        Some(RendererEvent::Message(
            crate::diff::Message::SetDiffHorizontalScroll(position)
        ))
            if position == horizontal_maximum
    ));
    model.diff_scroll = vertical_target;
    let skeleton = renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    assert!(skeleton.viewport_transition.is_none());
    if !skeleton.syntax_ready {
        wait_for_syntax_ready(&mut renderer, &model);
    }
    assert_eq!(model.diff_scroll, vertical_target);
}

#[test]
fn horizontal_scrollbar_tracks_only_the_visible_vertical_slice() {
    let mut model = model();
    let mut patch = String::from("@@ -1,100 +1,100 @@\n-old first\n+new first\n");
    for line in 0..100 {
        if line == 80 {
            writeln!(patch, " {}", "wide-content-".repeat(20)).unwrap();
        } else {
            writeln!(patch, " short line {line}").unwrap();
        }
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();
    let area = Rect::new(0, 0, 100, 30);

    let top = renderer.prepare_frame(&model, area);
    assert_eq!(top.maximum_horizontal_scroll, 0);

    model.diff_scroll = 70;
    let wide = renderer.prepare_frame(&model, area);
    assert!(wide.maximum_horizontal_scroll > 0);
    model.diff_horizontal_scroll = wide.maximum_horizontal_scroll;

    model.diff_scroll = 0;
    let top_again = renderer.prepare_frame(&model, area);
    assert_eq!(top_again.maximum_horizontal_scroll, 0);
    model.clamp_diff_scroll(
        top_again.maximum_vertical_scroll,
        top_again.maximum_horizontal_scroll,
    );
    assert_eq!(model.diff_horizontal_scroll, 0);
}

#[test]
fn uncached_scroll_uses_one_viewport_and_skeleton_until_syntax_is_ready() {
    let mut model = model();
    let mut patch = String::from("@@ -1,700 +1,700 @@\n");
    for line in 1..=700 {
        if matches!(line, 2 | 620) {
            writeln!(patch, "-let value_{line} = 0;").unwrap();
            writeln!(patch, "+let value_{line} = {line};").unwrap();
        } else {
            writeln!(patch, " let value_{line} = {line};").unwrap();
        }
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();
    let area = Rect::new(0, 0, 100, 30);
    diff_lines(&mut renderer, &model, 0);

    model.diff_scroll = 600;
    let first = renderer.prepare_frame(&model, area);
    assert!(!first.syntax_ready);
    assert!(first.viewport_transition.is_none());
    let skeleton = renderer.diff_skeleton_lines(80, model.diff_scroll, 20);
    assert!(!skeleton.is_empty());
    insta::assert_debug_snapshot!("uncached_skeleton", skeleton);
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();
    let changes = &renderer.highlighted.as_ref().unwrap().inline_changes;
    let marker_row = renderer.scrollbars.vertical_area.y
        + overview_position(
            changes[1].first,
            renderer.scrollbars.rows,
            renderer.scrollbars.vertical_area.height,
        );
    let marker_column = renderer.scrollbars.vertical_area.x.saturating_add(1);
    assert_eq!(
        terminal.backend().buffer()[(marker_column, marker_row)].symbol(),
        diffo_ui::icons::CHANGE_MARKER
    );
    assert!(renderer.hunk_buttons.previous.is_some());

    model.diff_scroll = 650;
    let newest = renderer.prepare_frame(&model, area);
    assert!(!newest.syntax_ready);
    wait_for_syntax_ready(&mut renderer, &model);
    assert_eq!(model.diff_scroll, 650);
    assert!(renderer.syntax_ready_for_viewport(DiffViewMode::Inline, 650));

    let computations = renderer.highlight_computations;
    model.diff_scroll = 2;
    let revisited = renderer.prepare_frame(&model, area);
    assert!(revisited.syntax_ready);
    assert_eq!(renderer.highlight_computations, computations);
}

#[test]
fn hunk_markers_have_a_separate_clickable_rail_beside_the_scrollbar() {
    let mut model = model();
    let mut patch = String::from("@@ -1,100 +1,100 @@\n");
    for line in 1..=100 {
        if matches!(line, 2 | 90) {
            writeln!(patch, "-old {line}").unwrap();
            writeln!(patch, "+new {line}").unwrap();
        } else {
            writeln!(patch, " line {line}").unwrap();
        }
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    wait_for_syntax_ready(&mut renderer, &model);
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    let changes = &renderer.highlighted.as_ref().unwrap().inline_changes;
    let target = changes[1].first;
    let marker_column = renderer.scrollbars.vertical_area.x.saturating_add(1);
    let marker_row = renderer.scrollbars.vertical_area.y
        + overview_position(
            target,
            renderer.scrollbars.rows,
            renderer.scrollbars.vertical_area.height,
        );
    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: marker_column,
        row: marker_row,
        modifiers: KeyModifiers::NONE,
    });

    insta::assert_debug_snapshot!(buffer_region(
        terminal.backend().buffer(),
        Rect::new(
            renderer.scrollbars.vertical_area.x,
            renderer.scrollbars.vertical_area.y,
            2,
            renderer.scrollbars.vertical_area.height,
        ),
    ));
    assert_eq!(
        renderer.change_at_marker(renderer.scrollbars.vertical_area.x, marker_row, &model),
        None
    );
    assert_eq!(
        renderer.scrollbar_at(renderer.scrollbars.vertical_area.x, marker_row),
        Some(crate::diff::ScrollbarAxis::Vertical)
    );
    assert_eq!(
        renderer.map_event(&click, &model, Rect::new(0, 0, 100, 30)),
        Some(RendererEvent::Message(
            crate::diff::Message::JumpDiffToPosition(target)
        ))
    );
    let old_scroll = model.diff_scroll;
    let pending = renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    let transition = pending
        .viewport_transition
        .unwrap_or_else(|| wait_for_viewport_transition(&mut renderer, &model));
    assert_eq!(model.diff_scroll, old_scroll);
    assert_eq!(transition.vertical, target);
}

#[test]
fn large_hunk_buttons_are_fixed_and_do_not_wrap() {
    let mut model = model();
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = large_hunk_patch();
    let area = Rect::new(0, 0, 100, 30);
    let mut renderer = Renderer::new();
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    let top_preparation = renderer.prepare_frame(&model, area);
    wait_for_syntax_ready(&mut renderer, &model);
    let top_viewport =
        renderer.diff_viewport_metrics(model.diff_view_mode, area, model.diff_scroll);
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    assert!(renderer.hunk_buttons.previous.is_none());
    let next_area = renderer.hunk_buttons.next.expect("next button");
    let next_target = renderer
        .change_jump(&model, area, true)
        .expect("next target");
    assert_jump_event(
        &mut renderer,
        &Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)),
        &model,
        area,
        next_target,
    );
    insta::assert_debug_snapshot!(
        "next_button",
        buffer_region(terminal.backend().buffer(), next_area)
    );
    assert!(renderer.scrollbars.horizontal_area.height > 0);
    assert_eq!(next_area.bottom(), renderer.scrollbars.horizontal_area.y);
    assert_jump_event(
        &mut renderer,
        &mouse_at(MouseEventKind::Down(MouseButton::Left), next_area),
        &model,
        area,
        next_target,
    );

    let transition = renderer
        .prepare_frame(&model, area)
        .viewport_transition
        .expect("button jump must commit in one frame");
    assert_eq!(transition.vertical, next_target);
    assert!(renderer.submitted.is_empty());
    model.diff_scroll = transition.vertical;
    let middle_viewport =
        renderer.diff_viewport_metrics(model.diff_view_mode, area, model.diff_scroll);
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();
    let previous_area = renderer.hunk_buttons.previous.expect("previous button");
    let previous_target = renderer
        .change_jump(&model, area, false)
        .expect("previous target");
    insta::assert_debug_snapshot!(
        "previous_button",
        buffer_region(terminal.backend().buffer(), previous_area)
    );
    assert_eq!(previous_area.y, area.y.saturating_add(1));
    assert!(renderer.hunk_buttons.next.is_some());
    assert_jump_event(
        &mut renderer,
        &mouse_at(MouseEventKind::Down(MouseButton::Left), previous_area),
        &model,
        area,
        previous_target,
    );

    model.diff_scroll = renderer
        .highlighted
        .as_ref()
        .unwrap()
        .inline_changes
        .last()
        .map(|change| change.first)
        .unwrap();
    let end_preparation = renderer.prepare_frame(&model, area);
    let end_viewport =
        renderer.diff_viewport_metrics(model.diff_view_mode, area, model.diff_scroll);
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();
    assert!(renderer.hunk_buttons.next.is_none());
    assert_eq!(
        end_preparation.maximum_vertical_scroll,
        top_preparation.maximum_vertical_scroll
    );
    assert_eq!(top_viewport.content_area.y, middle_viewport.content_area.y);
    assert_eq!(middle_viewport.content_area.y, end_viewport.content_area.y);
    assert_eq!(
        viewport_control_bottom(top_viewport),
        viewport_control_bottom(middle_viewport)
    );
    assert_eq!(
        viewport_control_bottom(middle_viewport),
        viewport_control_bottom(end_viewport)
    );
    assert_eq!(
        renderer.hunk_button_direction_at(next_area.x, next_area.y),
        None
    );
}

#[test]
fn inline_and_side_by_side_navigation_use_their_own_region_bounds() {
    let mut patch = String::from("@@ -1,20 +1,20 @@\n context\n");
    for line in 0..10 {
        writeln!(patch, "-old {line}").unwrap();
    }
    for line in 0..10 {
        writeln!(patch, "+new {line}").unwrap();
    }
    for line in 0..9 {
        writeln!(patch, " context {line}").unwrap();
    }
    let area = Rect::new(0, 0, 100, 16);

    let mut inline_model = model();
    inline_model.snapshot.files[0]
        .unstaged
        .as_mut()
        .unwrap()
        .text = patch.clone();
    let mut inline_renderer = Renderer::new();
    inline_renderer.prepare_frame(&inline_model, area);
    let inline = inline_renderer.diff_viewport_metrics_at(DiffViewMode::Inline, area, 0);

    let mut side_model = model();
    side_model.diff_view_mode = DiffViewMode::SideBySide;
    side_model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut side_renderer = Renderer::new();
    side_renderer.prepare_frame(&side_model, area);
    let side = side_renderer.diff_viewport_metrics_at(DiffViewMode::SideBySide, area, 0);

    assert_eq!(inline.viewport_rows, side.viewport_rows);
    assert!(inline.next_change.is_some());
    assert_eq!(side.next_change, None);
    assert_eq!(
        inline_renderer.highlighted.as_ref().unwrap().inline_changes[0],
        diffo_diff::ChangeRegion { first: 2, last: 21 }
    );
    assert_eq!(
        side_renderer
            .highlighted
            .as_ref()
            .unwrap()
            .side_by_side_changes[0],
        diffo_diff::ChangeRegion { first: 2, last: 11 }
    );
}

fn large_hunk_patch() -> String {
    let mut patch = String::from("@@ -1,100 +1,100 @@\n");
    for line in 1..=100 {
        if matches!(line, 2 | 50 | 90) {
            writeln!(patch, "-old {line}").unwrap();
            writeln!(patch, "+new {line}").unwrap();
        } else if line == 10 {
            writeln!(patch, " {}", "wide-content-".repeat(20)).unwrap();
        } else {
            writeln!(patch, " line {line}").unwrap();
        }
    }
    patch
}

mod transitions;
