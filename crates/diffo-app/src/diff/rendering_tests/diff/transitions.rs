use super::*;

#[test]
fn ready_discrete_jump_commits_in_one_frame_without_preparation() {
    let mut model = model();
    model.snapshot.files[0].unstaged.as_mut().unwrap().text =
        "@@ -1,4 +1,4 @@\n-old\n+new\n context\n-old two\n+new two\n".to_owned();
    let area = Rect::new(0, 0, 100, 30);
    let mut renderer = Renderer::new();
    renderer.prepare_frame(&model, area);
    wait_for_syntax_ready(&mut renderer, &model);
    let computations = renderer.highlight_computations;
    let target = model.diff_scroll;
    assert!(renderer.syntax_ready_for_viewport(model.diff_view_mode, target));
    renderer.requested_navigation_target = Some(target);
    let preparation = renderer.prepare_frame(&model, area);

    assert_eq!(preparation.viewport_transition.unwrap().vertical, target);
    assert_eq!(renderer.highlight_computations, computations);
    assert!(renderer.submitted.is_empty());
}

#[test]
fn passive_mouse_movement_does_not_change_hunk_buttons_or_request_actions() {
    let mut model = model();
    let mut patch = String::from("@@ -1,100 +1,100 @@\n");
    for line in 1..=100 {
        if matches!(line, 2 | 50 | 90) {
            writeln!(patch, "-old {line}").unwrap();
            writeln!(patch, "+new {line}").unwrap();
        } else {
            writeln!(patch, " line {line}").unwrap();
        }
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    model.diff_scroll = 50;
    let area = Rect::new(0, 0, 100, 30);
    let mut renderer = Renderer::new();
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    renderer.prepare_frame(&model, area);
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    let previous = renderer.hunk_buttons.previous.expect("previous button").0;
    let next = renderer.hunk_buttons.next.expect("next button").0;
    let before_movement = terminal.backend().buffer().clone();
    let positions = [
        previous,
        Rect::new(previous.right().saturating_sub(1), previous.y, 1, 1),
        next,
        Rect::new(next.right().saturating_sub(1), next.y, 1, 1),
        Rect::new(area.x, area.y, 1, 1),
    ];

    for _ in 0..100 {
        for position in positions {
            assert_eq!(
                renderer.map_event(&mouse_at(MouseEventKind::Moved, position), &model, area),
                None
            );
        }
    }
    terminal
        .draw(|frame| renderer.render(frame, &model))
        .unwrap();

    assert_eq!(
        terminal.backend().buffer(),
        &before_movement,
        "passive movement must produce zero changed terminal cells"
    );
}

#[test]
fn prepares_view_modes_lazily_caches_them_and_invalidates_changed_patch() {
    let mut renderer = Renderer::new();
    let mut model = model();

    diff_lines(&mut renderer, &model, 0);
    model.diff_view_mode = DiffViewMode::SideBySide;
    diff_lines(&mut renderer, &model, 0);
    assert_eq!(renderer.highlight_computations, 2);

    model.diff_view_mode = DiffViewMode::Inline;
    diff_lines(&mut renderer, &model, 0);
    assert_eq!(renderer.highlight_computations, 2);

    model.snapshot.files[0]
        .unstaged
        .as_mut()
        .expect("unstaged diff")
        .text
        .push_str("\\ No newline at end of file\n");
    diff_lines(&mut renderer, &model, 0);
    assert_eq!(renderer.highlight_computations, 3);
}

#[test]
fn view_mode_and_reset_viewport_commit_together() {
    let mut renderer = Renderer::new();
    let mut model = model();
    let mut patch = String::from("@@ -1,700 +1,700 @@\n");
    for line in 1..=700 {
        if line == 600 {
            writeln!(patch, "-let old_target = {line};").unwrap();
            writeln!(patch, "+let new_target = {line};").unwrap();
        } else {
            writeln!(patch, " let context_{line} = {line};").unwrap();
        }
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    diff_lines(&mut renderer, &model, 0);
    assert_eq!(
        renderer.highlighted.as_ref().unwrap().key.mode,
        DiffViewMode::Inline
    );

    model.diff_scroll = 10;
    model.diff_horizontal_scroll = 5;
    model.diff_view_mode = DiffViewMode::SideBySide;
    let pending = renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    assert!(pending.viewport_transition.is_none());
    assert_eq!(
        renderer.highlighted.as_ref().unwrap().key.mode,
        DiffViewMode::Inline
    );
    assert_eq!((model.diff_scroll, model.diff_horizontal_scroll), (10, 5));

    let transition = wait_for_viewport_transition(&mut renderer, &model);
    assert_eq!((transition.vertical, transition.horizontal), (0, 0));
    let cache = renderer.highlighted.as_ref().unwrap();
    assert_eq!(cache.key.mode, DiffViewMode::SideBySide);
    assert!(cache.inline.is_empty());
    assert!(!cache.side_by_side.is_empty());
}

#[test]
fn reuses_a_prepared_buffer_after_visiting_another_file() {
    let mut renderer = Renderer::new();
    let mut model = model();
    model.snapshot.files.push(FileState {
        path: PathBuf::from("src/second.rs"),
        old_path: None,
        kind: ChangeKind::Modified,
        staged: None,
        unstaged: Some(FileDiff {
            text: "@@ -1 +1 @@\n-let second = 1;\n+let second = 2;\n".to_owned(),
        }),
    });

    diff_lines(&mut renderer, &model, 0);
    assert_eq!(renderer.highlight_computations, 1);
    model.select_next();
    diff_lines(&mut renderer, &model, 0);
    assert_eq!(renderer.highlight_computations, 2);
    model.select_previous();
    diff_lines(&mut renderer, &model, 0);

    assert_eq!(renderer.highlight_computations, 2);
    assert_eq!(
        renderer.highlighted.as_ref().unwrap().key.file.path,
        PathBuf::from("src/main.rs")
    );
}

#[test]
fn syntax_highlighting_uses_a_strict_ten_thousand_file_line_limit() {
    let below_limit = diffo_diff::parse_unified_patch(
        "@@ -9999 +9999 @@\n-pub const VALUE: usize = 1;\n+pub const VALUE: usize = 2;\n",
    )
    .unwrap();
    let at_limit = diffo_diff::parse_unified_patch(
        "@@ -10000 +10000 @@\n-pub const VALUE: usize = 1;\n+pub const VALUE: usize = 2;\n",
    )
    .unwrap();

    assert_eq!(diff_file_lines(&below_limit), 9_999);
    assert!(should_syntax_highlight(&below_limit));
    assert_eq!(diff_file_lines(&at_limit), 10_000);
    assert!(!should_syntax_highlight(&at_limit));
}

#[test]
fn initial_highlighting_is_bounded_around_the_first_change() {
    let mut model = model();
    let mut patch = String::from("@@ -1,9999 +1,9999 @@\n");
    for line in 1..=9_999 {
        if line == 9_000 {
            writeln!(patch, "-pub const OLD_TARGET: usize = 1;").unwrap();
            writeln!(patch, "+pub const NEW_TARGET: usize = 2;").unwrap();
        } else {
            writeln!(patch, " pub const LINE_{line}: usize = {line};").unwrap();
        }
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();

    diff_lines(&mut renderer, &model, 0);

    let cache = renderer.highlighted.as_ref().unwrap();
    assert!(
        cache
            .highlighted_old_coverage
            .iter()
            .any(|range| range.contains(9_000))
    );
    assert!(
        cache
            .highlighted_new_coverage
            .iter()
            .any(|range| range.contains(9_000))
    );
    assert!(cache.highlighted_lines_processed < 800);
    assert!(!cache.highlighted.new.contains_key(&1));
    assert!(cache.highlighted.new.contains_key(&9_000));
    assert!(
        cache
            .highlighted_new_coverage
            .iter()
            .any(|range| range.contains(8_990)),
        "initial coverage must include an equal opportunity to scroll upward"
    );
}

#[test]
fn syntax_prefetch_size_ignores_scroll_direction() {
    assert_eq!(highlight_prefetch_viewports(100, 96, 20), 7);
    assert_eq!(highlight_prefetch_viewports(100, 104, 20), 7);
    assert_eq!(highlight_prefetch_viewports(100, 80, 20), 13);
    assert_eq!(highlight_prefetch_viewports(100, 120, 20), 13);
    assert_eq!(highlight_prefetch_viewports(100, 100, 20), 3);
}

#[test]
fn lifts_low_contrast_theme_colors_on_diff_backgrounds() {
    let monokai_comment = Rgb {
        red: 117,
        green: 113,
        blue: 94,
    };
    for kind in [RowKind::Removed, RowKind::Added] {
        let adjusted = contrasting_foreground(monokai_comment, kind);
        let background = diff_background_rgb(kind).expect("changed row has a background");

        assert!(contrast_ratio(adjusted, background) >= 4.5);
    }
    assert_eq!(
        contrasting_foreground(monokai_comment, RowKind::Context),
        monokai_comment
    );
}

#[test]
#[ignore = "manual performance measurement"]
fn measures_large_diff_rendering() {
    let mut model = model();
    let mut patch = String::from("@@ -0,0 +1,100000 @@\n");
    for index in 0..100_000 {
        writeln!(patch, "+pub const ITEM_{index}: usize = {index};").unwrap();
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();

    let started = Instant::now();
    renderer.prepare_frame(&model, Rect::new(0, 0, 180, 60));
    let loading = renderer.diff_lines(&model, 160, 0, 50);
    let enqueue = started.elapsed();
    assert!(loading.is_empty());
    let started = Instant::now();
    let lines = loop {
        renderer.prepare_frame(&model, Rect::new(0, 0, 180, 60));
        let lines = renderer.diff_lines(&model, 160, 0, 50);
        if !renderer.is_preparing() {
            break lines;
        }
        sleep(Duration::from_millis(1));
    };
    let prepared = started.elapsed();
    let started = Instant::now();
    for row in (0..10_000).step_by(50) {
        assert_eq!(renderer.diff_lines(&model, 160, row, 50).len(), 50);
    }
    let cached = started.elapsed();

    eprintln!(
        "100k enqueue={enqueue:?} background_prepare={prepared:?} cached_200_viewports={cached:?}"
    );
    assert_eq!(lines.len(), 50);
    assert_eq!(renderer.highlight_computations, 0);
}

#[test]
#[ignore = "manual file-open performance measurement"]
fn measures_bounded_9999_line_file_open() {
    let mut model = model();
    let mut patch = String::from("@@ -1,9999 +1,9999 @@\n");
    for line in 1..=9_999 {
        if line == 9_000 {
            writeln!(patch, "-pub const OLD_TARGET: usize = 1;").unwrap();
            writeln!(patch, "+pub const PERF_TARGET_09000: usize = 2;").unwrap();
        } else {
            writeln!(patch, " pub const LINE_{line}: usize = {line};").unwrap();
        }
    }
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = patch;
    let mut renderer = Renderer::new();
    let area = Rect::new(0, 0, 100, 30);
    let started = Instant::now();
    let transition = loop {
        let preparation = renderer.prepare_frame(&model, area);
        if let Some(transition) = preparation.viewport_transition {
            break transition;
        }
        sleep(Duration::from_millis(1));
    };
    let elapsed = started.elapsed();
    let cache = renderer.highlighted.as_ref().unwrap();

    eprintln!("bounded 9,999-line open={elapsed:?}");
    assert!(transition.vertical > 8_900);
    assert!(cache.highlighted_lines_processed < 800);
}
