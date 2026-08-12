use super::*;
use crate::diff::prepare::{
    prepare_diff,
    state::{DiffKey, PrepareOutcome, PrepareRequest},
};

const PREPARATION_TIMEOUT: Duration = Duration::from_secs(5);

fn diff_lines(
    renderer: &mut Renderer,
    model: &Model,
    first_row: usize,
) -> Vec<ratatui::text::Line<'static>> {
    let deadline = Instant::now() + PREPARATION_TIMEOUT;
    loop {
        renderer.prepare_frame(model, Rect::new(0, 0, 100, 30));
        let lines = renderer.diff_lines(model, 80, first_row, 100);
        if !renderer.is_preparing() {
            return lines;
        }
        assert!(Instant::now() < deadline, "diff preparation timed out");
        sleep(Duration::from_millis(1));
    }
}

fn wait_for_viewport_transition(
    renderer: &mut Renderer,
    model: &Model,
) -> crate::diff::ViewportTransition {
    let deadline = Instant::now() + PREPARATION_TIMEOUT;
    loop {
        let preparation = renderer.prepare_frame(model, Rect::new(0, 0, 100, 30));
        if let Some(viewport) = preparation.viewport_transition {
            return viewport;
        }
        assert!(Instant::now() < deadline, "viewport preparation timed out");
        sleep(Duration::from_millis(1));
    }
}

fn wait_for_syntax_ready(renderer: &mut Renderer, model: &Model) {
    let deadline = Instant::now() + PREPARATION_TIMEOUT;
    loop {
        let preparation = renderer.prepare_frame(model, Rect::new(0, 0, 100, 30));
        if preparation.syntax_ready {
            return;
        }
        assert!(Instant::now() < deadline, "syntax preparation timed out");
        sleep(Duration::from_millis(1));
    }
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

    let deadline = Instant::now() + PREPARATION_TIMEOUT;
    let committed = loop {
        let preparation = renderer.prepare_frame(&model, area);
        if preparation.viewport_transition.is_some() {
            break preparation;
        }
        assert!(
            Instant::now() < deadline,
            "second diff preparation timed out"
        );
        sleep(Duration::from_millis(1));
    };
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
    let Some(RendererEvent::Message(crate::diff::Message::JumpDiffToPosition(vertical_target))) =
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
    let old_scroll = model.diff_scroll;
    let pending = renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    assert!(pending.syntax_ready);
    let committed = pending
        .viewport_transition
        .unwrap_or_else(|| wait_for_viewport_transition(&mut renderer, &model));
    assert_eq!(model.diff_scroll, old_scroll);
    assert_eq!(committed.vertical, vertical_target);
}

#[test]
fn horizontal_scrollbar_tracks_only_the_visible_vertical_slice() {
    for mode in [DiffViewMode::Inline, DiffViewMode::SideBySide] {
        let mut model = model();
        model.diff_view_mode = mode;
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
        let diff_area = horizontal_panes(main_area(area), model.file_pane_percent)[1];

        let top = renderer.prepare_frame(&model, area);
        let top_viewport = renderer.diff_viewport_metrics(mode, diff_area, model.diff_scroll);
        assert_eq!(top.maximum_horizontal_scroll, 0);
        assert!(top_viewport.horizontal_area.is_empty());

        model.diff_scroll = 70;
        let wide = renderer.prepare_frame(&model, area);
        let wide_viewport = renderer.diff_viewport_metrics(mode, diff_area, model.diff_scroll);
        assert!(wide.maximum_horizontal_scroll > 0);
        assert!(!wide_viewport.horizontal_area.is_empty());
        assert_eq!(top_viewport.content_area, wide_viewport.content_area);
        assert_eq!(
            top_viewport.maximum_vertical_scroll,
            wide_viewport.maximum_vertical_scroll
        );
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
}

#[test]
fn page_movement_uses_the_full_diff_viewport() {
    let mut model = model();
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = large_hunk_patch();
    let area = Rect::new(0, 0, 100, 30);
    let mut renderer = Renderer::new();
    renderer.prepare_frame(&model, area);
    let diff_area = horizontal_panes(main_area(area), model.file_pane_percent)[1];
    let viewport =
        renderer.diff_viewport_metrics(model.diff_view_mode, diff_area, model.diff_scroll);
    let event = Event::Key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));

    assert_eq!(
        renderer.map_event(&event, &model, area),
        Some(RendererEvent::Message(
            crate::diff::Message::JumpDiffToPosition(viewport.viewport_rows)
        ))
    );
}

#[test]
fn side_by_side_horizontal_pan_keeps_gutters_and_divider_fixed() {
    let mut model = model();
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = format!(
        "@@ -1 +1 @@\n-OLD_{}\n+NEW_{}RIGHT_EDGE\n",
        "x".repeat(80),
        "y".repeat(80),
    );
    model.toggle_diff_view();
    let mut renderer = Renderer::new();
    diff_lines(&mut renderer, &model, 0);
    let area = Rect::new(0, 0, 100, 30);
    let preparation = renderer.prepare_frame(&model, area);

    assert!(preparation.maximum_horizontal_scroll > 0);
    let diff_area = horizontal_panes(main_area(area), model.file_pane_percent)[1];
    let viewport =
        renderer.diff_viewport_metrics(DiffViewMode::SideBySide, diff_area, model.diff_scroll);
    assert!(!viewport.horizontal_area.is_empty());
    model.diff_horizontal_scroll = preparation.maximum_horizontal_scroll;
    let row = &renderer.diff_lines(
        &model,
        viewport.content_area.width,
        model.diff_scroll,
        viewport.viewport_rows,
    )[1];
    let text = row
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();

    assert!(text.starts_with("   1 "));
    assert!(text.contains(" │    1 "));
    assert!(text.ends_with("RIGHT_EDGE"));
}

#[test]
fn uncached_scroll_keeps_the_committed_viewport_until_syntax_is_ready_in_both_directions() {
    let mut patch = String::from("@@ -1,700 +1,700 @@\n");
    for line in 1..=700 {
        if line == 350 {
            writeln!(patch, "-let value_{line} = 0;").unwrap();
            writeln!(patch, "+let value_{line} = {line};").unwrap();
        } else {
            writeln!(patch, " let value_{line} = {line};").unwrap();
        }
    }
    let area = Rect::new(0, 0, 100, 30);
    for target in [100, 600] {
        let mut model = model();
        model.snapshot.files[0]
            .unstaged
            .as_mut()
            .unwrap()
            .text
            .clone_from(&patch);
        let mut renderer = Renderer::new();
        let initial = renderer.prepare_frame(&model, area);
        let initial = initial
            .viewport_transition
            .unwrap_or_else(|| wait_for_viewport_transition(&mut renderer, &model));
        model.set_diff_viewport(initial.vertical, initial.horizontal);
        if target == 100 {
            assert!(initial.vertical > target);
        } else {
            assert!(initial.vertical < target);
        }

        assert_eq!(
            renderer.vertical_message(crate::diff::Message::SetDiffScroll(target), &model),
            crate::diff::Message::JumpDiffToPosition(target)
        );
        let pending = renderer.prepare_frame(&model, area);
        assert!(pending.viewport_transition.is_none());
        assert!(
            pending.syntax_ready,
            "the committed viewport must remain fully rendered"
        );
        assert_eq!(model.diff_scroll, initial.vertical);
        assert!(
            renderer
                .diff_lines(&model, 80, model.diff_scroll, 20)
                .iter()
                .any(|line| line.width() > 7),
            "pending scroll rendered gutter-only skeleton rows"
        );

        let committed = wait_for_viewport_transition(&mut renderer, &model);
        assert_eq!(committed.vertical, target);
        assert!(renderer.syntax_ready_for_viewport(DiffViewMode::Inline, target));
    }
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
    let marker = changes[1].first;
    let target = marker.saturating_sub(1);
    let marker_column = renderer.scrollbars.vertical_area.x.saturating_add(1);
    let marker_row = renderer.scrollbars.vertical_area.y
        + overview_position(
            marker,
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

fn assert_bold_row(buffer: &Buffer, area: Rect) {
    assert!(
        (area.x..area.right())
            .all(|column| { buffer[(column, area.y)].modifier.contains(Modifier::BOLD) })
    );
}

#[test]
fn change_navigation_links_overlay_fixed_edge_rows_and_activate() {
    let mut model = model();
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = large_hunk_patch();
    let area = Rect::new(0, 0, 100, 30);
    let mut renderer = Renderer::new();
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    let top_preparation = renderer.prepare_frame(&model, area);
    wait_for_syntax_ready(&mut renderer, &model);
    let diff_area = horizontal_panes(main_area(area), model.file_pane_percent)[1];
    let top_viewport =
        renderer.diff_viewport_metrics(model.diff_view_mode, diff_area, model.diff_scroll);
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();
    assert!(renderer.change_warnings.previous.is_none());
    let next_area = renderer.change_warnings.next.expect("next warning");
    let next_target = renderer
        .change_jump(&model, area, true)
        .expect("next target");
    insta::assert_debug_snapshot!(
        "next_warning",
        buffer_region(terminal.backend().buffer(), next_area)
    );
    assert_bold_row(terminal.backend().buffer(), next_area);
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
        .expect("link jump must commit in one frame");
    assert_eq!(transition.vertical, next_target);
    assert!(renderer.submitted.is_empty());
    model.diff_scroll = transition.vertical;
    let middle_viewport =
        renderer.diff_viewport_metrics(model.diff_view_mode, diff_area, model.diff_scroll);
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();
    let previous_area = renderer.change_warnings.previous.expect("previous warning");
    insta::assert_debug_snapshot!(
        "previous_warning",
        buffer_region(terminal.backend().buffer(), previous_area)
    );
    assert_eq!(previous_area.y, area.y.saturating_add(1));
    assert!(renderer.change_warnings.next.is_some());
    assert_bold_row(terminal.backend().buffer(), previous_area);
    let previous_target = renderer
        .change_jump(&model, area, false)
        .expect("previous target");
    assert_jump_event(
        &mut renderer,
        &mouse_at(MouseEventKind::Down(MouseButton::Left), previous_area),
        &model,
        area,
        previous_target,
    );
    let previous_transition = renderer
        .prepare_frame(&model, area)
        .viewport_transition
        .expect("previous link jump must commit in one frame");
    assert_eq!(previous_transition.vertical, previous_target);

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
        renderer.diff_viewport_metrics(model.diff_view_mode, diff_area, model.diff_scroll);
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();
    assert!(renderer.change_warnings.next.is_none());
    assert_eq!(
        end_preparation.maximum_vertical_scroll,
        top_preparation.maximum_vertical_scroll
    );
    assert_eq!(top_viewport.content_area, middle_viewport.content_area);
    assert_eq!(middle_viewport.content_area, end_viewport.content_area);
    assert_eq!(
        renderer.map_event(
            &mouse_at(MouseEventKind::Down(MouseButton::Left), next_area),
            &model,
            area,
        ),
        None
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

mod rails;
mod transitions;
