use super::*;
use crate::diff::{FileKey, ReviewSelection};
use ratatui::text::Line;
use std::sync::Arc;

#[test]
fn keeps_the_previous_selection_visible_until_a_complete_change_is_prepared() {
    let model = model();
    let mut renderer = Renderer::new();
    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    let previous = renderer.displayed_key().cloned().expect("initial file");
    let patch = Arc::<str>::from("@@ -1 +1 @@\n-old\n+new\n");
    let complete = DiffKey {
        selection: ReviewSelection::CompleteChange("commit:aaaaaaaa".to_owned()),
        title: Line::raw(" aaaaaaa · complete change "),
        empty_message: "Commit contains no file changes.",
        patch: Arc::clone(&patch),
        mark_conflicts: false,
        mode: DiffViewMode::Hunk,
        hunk_segments: Some(Arc::from([crate::diff::ReviewHunkSegment {
            selection: ReviewSelection::File(FileKey {
                path: PathBuf::from("file.txt"),
                area: ChangeArea::Unstaged,
            }),
            patch,
            mark_conflicts: false,
        }])),
    };
    let outcome = PrepareOutcome {
        key: complete.clone(),
        target_scroll: None,
        cache: prepare_diff(
            &PrepareRequest {
                key: complete.clone(),
                viewport_rows: 20,
                mode: DiffViewMode::Hunk,
                target_scroll: None,
                prefetch_viewports: 3,
            },
            &renderer.highlighter,
        ),
    };
    renderer.requested = Some(complete.clone());
    renderer.submitted = vec![(complete.clone(), None)];

    assert_eq!(renderer.displayed_key(), Some(&previous));
    renderer
        .accept_prepared_outcome(Some(&complete), outcome)
        .expect("complete change should commit atomically");
    assert_eq!(renderer.displayed_key(), Some(&complete));
}

#[test]
fn discards_a_stale_prepared_buffer_before_committing_the_latest() {
    let mut model = model();
    for path in ["src/b.rs", "src/c.rs"] {
        model.snapshot.files.push(FileState {
            path: PathBuf::from(path),
            old_path: None,
            kind: ChangeKind::Modified,
            staged: None,
            unstaged: Some(FileDiff {
                text: format!("@@ -1 +1 @@\n-old {path}\n+new {path}\n"),
            }),
        });
    }
    let mut renderer = Renderer::new();
    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    let initial = renderer.displayed_key().cloned().unwrap();

    model.select_next();
    let stale_key = renderer.requested_key(&model).unwrap();
    model.select_next();
    let latest_key = renderer.requested_key(&model).unwrap();
    let outcome = |key: DiffKey| {
        let request = PrepareRequest {
            key: key.clone(),
            viewport_rows: 28,
            mode: model.diff_view_mode,
            target_scroll: None,
            prefetch_viewports: diffo_ui::text_view::syntax_prefetch_viewports(0, 0, 20),
        };
        PrepareOutcome {
            key,
            target_scroll: None,
            cache: prepare_diff(&request, &renderer.highlighter),
        }
    };
    let stale = outcome(stale_key.clone());
    let latest = outcome(latest_key.clone());
    renderer.requested = Some(latest_key.clone());
    renderer.submitted = vec![(stale_key, None), (latest_key.clone(), None)];

    assert!(
        renderer
            .accept_prepared_outcome(Some(&latest_key), stale)
            .is_none()
    );
    assert_eq!(renderer.displayed_key(), Some(&initial));
    assert_eq!(renderer.submitted, vec![(latest_key.clone(), None)]);

    let commit = renderer
        .accept_prepared_outcome(Some(&latest_key), latest)
        .expect("latest prepared buffer must commit");
    assert!(commit.target_scroll.is_none());
    assert_eq!(renderer.displayed_key(), Some(&latest_key));
    assert!(renderer.submitted.is_empty());
}

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
    renderer.vertical_scroll.request(
        diffo_ui::text_view::ScrollCommand::Vertical(target),
        model.diff_scroll,
        diffo_ui::text_view::ViewportMetrics {
            maximum_vertical: usize::MAX,
            ..diffo_ui::text_view::ViewportMetrics::default()
        },
    );
    let preparation = renderer.prepare_frame(&model, area);

    assert_eq!(preparation.viewport_transition.unwrap().vertical, target);
    assert_eq!(renderer.highlight_computations, computations);
    assert!(renderer.submitted.is_empty());
}

#[test]
fn passive_mouse_movement_does_not_change_warnings_or_request_actions() {
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

    let previous = renderer.change_warnings.previous.expect("previous warning");
    let next = renderer.change_warnings.next.expect("next warning");
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
        renderer
            .highlighted
            .as_ref()
            .unwrap()
            .key
            .selection
            .file_key()
            .unwrap()
            .path,
        PathBuf::from("src/main.rs")
    );
}

#[test]
fn hunk_mode_compacts_all_files_and_file_selection_only_moves_the_viewport() {
    let mut renderer = Renderer::new();
    let mut model = model();
    model.diff_view_mode = DiffViewMode::Hunk;
    model.snapshot.files[0].unstaged.as_mut().unwrap().text =
        full_file_patch("src/main.rs", "FIRST_OLD", "FIRST_NEW");
    model.snapshot.files.push(FileState {
        path: PathBuf::from("src/second.rs"),
        old_path: None,
        kind: ChangeKind::Modified,
        staged: None,
        unstaged: Some(FileDiff {
            text: full_file_patch("src/second.rs", "SECOND_OLD", "SECOND_NEW"),
        }),
    });

    diff_lines(&mut renderer, &model, 0);
    let cache = renderer.highlighted.as_ref().unwrap();
    let text = cache
        .hunk
        .iter()
        .map(|row| row.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("FIRST_NEW"));
    assert!(text.contains("SECOND_NEW"));
    assert!(!text.contains("far context 050"));
    assert!(cache.syntax_highlighted);
    assert!(
        cache
            .hunk_highlighted
            .iter()
            .any(|highlighted| !highlighted.new.is_empty())
    );
    let revision = renderer.content_revision;
    let computations = renderer.highlight_computations;

    model.select_next();
    let transition = renderer
        .prepare_frame(&model, Rect::new(0, 0, 100, 30))
        .viewport_transition
        .expect("file focus should prepare a hunk target");

    assert!(transition.vertical > 0);
    assert_eq!(renderer.content_revision, revision);
    assert!(
        renderer
            .highlighted
            .as_ref()
            .unwrap()
            .hunk
            .iter()
            .any(|row| { row.text.contains("FIRST_NEW") })
    );
    assert_eq!(renderer.highlight_computations, computations);
}

#[test]
fn far_hunk_file_focus_waits_for_target_syntax_before_moving_the_viewport() {
    let mut renderer = Renderer::new();
    let mut model = model();
    model.diff_view_mode = DiffViewMode::Hunk;
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = compact_rust_patch("src/main.rs", 0);
    for index in 1..100 {
        let path = format!("src/file_{index:03}.rs");
        model.snapshot.files.push(FileState {
            path: PathBuf::from(&path),
            old_path: None,
            kind: ChangeKind::Modified,
            staged: None,
            unstaged: Some(FileDiff {
                text: compact_rust_patch(&path, index),
            }),
        });
    }
    diff_lines(&mut renderer, &model, 0);
    for _ in 1..100 {
        model.select_next();
    }
    let selection = ReviewSelection::File(model.selected.clone().unwrap());
    let target = crate::diff::review_document::hunk_focus_target(
        renderer.highlighted.as_ref().unwrap(),
        &selection,
    )
    .expect("last file should have a hunk target");
    assert!(!renderer.syntax_ready_for_viewport(DiffViewMode::Hunk, target));

    let pending = renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    assert!(pending.viewport_transition.is_none());
    assert!(pending.preparing);
    let requested = renderer.requested.clone().unwrap();
    assert_eq!(renderer.submitted, vec![(requested.clone(), Some(target))]);

    let outcome = renderer
        .prepare_rx
        .recv_timeout(PREPARATION_TIMEOUT)
        .expect("target syntax preparation should complete");
    assert_eq!(outcome.target_scroll, Some(target));
    renderer
        .accept_prepared_outcome(Some(&requested), outcome)
        .expect("target syntax should commit");
    assert_ne!(renderer.displayed_selection, renderer.requested_selection);

    let ready = renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    assert_eq!(ready.viewport_transition.unwrap().vertical, target);
    assert!(ready.syntax_ready);
    assert!(!ready.preparing);
    assert!(renderer.submitted.is_empty());
}

#[test]
fn hunk_focus_for_a_metadata_only_file_stays_inside_its_segment() {
    let mut renderer = Renderer::new();
    let mut model = model();
    model.diff_view_mode = DiffViewMode::Hunk;
    model.snapshot.files.push(FileState {
        path: PathBuf::from("src/renamed.rs"),
        old_path: Some(PathBuf::from("src/old.rs")),
        kind: ChangeKind::Renamed,
        staged: None,
        unstaged: Some(FileDiff {
            text: concat!(
                "diff --git a/src/old.rs b/src/renamed.rs\n",
                "similarity index 100%\n",
                "rename from src/old.rs\n",
                "rename to src/renamed.rs\n",
            )
            .to_owned(),
        }),
    });
    model.snapshot.files.push(FileState {
        path: PathBuf::from("src/third.rs"),
        old_path: None,
        kind: ChangeKind::Modified,
        staged: None,
        unstaged: Some(FileDiff {
            text: "@@ -1 +1 @@\n-old third\n+new third\n".to_owned(),
        }),
    });
    diff_lines(&mut renderer, &model, 0);
    model.select_next();
    let selection = ReviewSelection::File(model.selected.clone().unwrap());
    let cache = renderer.highlighted.as_ref().unwrap();
    let range = cache
        .hunk_targets
        .iter()
        .find_map(|(candidate, range)| (candidate == &selection).then_some(range.clone()))
        .unwrap();

    assert_eq!(
        crate::diff::review_document::hunk_focus_target(cache, &selection),
        Some(range.start)
    );
    assert!(
        cache
            .hunk_changes
            .iter()
            .all(|change| change.first < range.start || change.first >= range.end)
    );
    let transition = renderer
        .prepare_frame(&model, Rect::new(0, 0, 100, 30))
        .viewport_transition
        .expect("metadata-only file should have a focus target");
    assert_eq!(transition.vertical, range.start);
}

fn full_file_patch(path: &str, old: &str, new: &str) -> String {
    let mut contents =
        format!("diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,100 +1,100 @@\n");
    for line in 1..=100 {
        if line == 75 {
            writeln!(contents, "-{old}").unwrap();
            writeln!(contents, "+{new}").unwrap();
        } else {
            writeln!(contents, " far context {line:03}").unwrap();
        }
    }
    contents
}

fn compact_rust_patch(path: &str, index: usize) -> String {
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-fn old_{index}() {{}}\n+fn new_{index}() {{}}\n"
    )
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
fn repeated_scroll_input_accumulates_against_the_pending_target_in_both_directions() {
    let mut model = model();
    model.diff_scroll = 100;
    let mut renderer = Renderer::new();

    assert_eq!(
        renderer.vertical_message(crate::diff::Message::ScrollDiffPageUp(20), &model),
        crate::diff::Message::JumpDiffToPosition(80)
    );
    assert_eq!(
        renderer.vertical_message(crate::diff::Message::ScrollDiffPageUp(20), &model),
        crate::diff::Message::JumpDiffToPosition(60)
    );
    assert_eq!(
        renderer.vertical_message(crate::diff::Message::ScrollDiffPageDown(20), &model),
        crate::diff::Message::JumpDiffToPosition(80)
    );
    assert_eq!(model.diff_scroll, 100);
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
