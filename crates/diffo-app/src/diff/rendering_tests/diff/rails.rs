use super::*;

#[test]
fn hunk_view_uses_the_unified_projection() {
    let mut model = model();
    model.diff_view_mode = DiffViewMode::Hunk;
    let mut renderer = Renderer::new();

    let lines = diff_lines(&mut renderer, &model, 0);

    assert!(lines.iter().any(|line| line.to_string().starts_with("@@ ")));
    assert!(lines.iter().any(|line| line.to_string().starts_with('-')));
    assert!(lines.iter().any(|line| line.to_string().starts_with('+')));
    assert!(
        !renderer
            .highlighted
            .as_ref()
            .unwrap()
            .hunk_changes
            .is_empty()
    );
}

#[test]
fn whole_block_navigation_uses_projection_specific_region_bounds() {
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
    assert_eq!(inline.next_change, None);
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

#[test]
fn change_warning_availability_does_not_move_the_diff_rails() {
    for mode in [
        DiffViewMode::Inline,
        DiffViewMode::SideBySide,
        DiffViewMode::Hunk,
    ] {
        assert_stable_diff_rails(mode);
    }
}

fn assert_stable_diff_rails(mode: DiffViewMode) {
    let mut model = model();
    model.diff_view_mode = mode;
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = navigation_layout_patch();
    let area = Rect::new(0, 0, 100, if mode == DiffViewMode::Hunk { 10 } else { 30 });
    let diff_area = horizontal_panes(main_area(area), model.file_pane_percent)[1];
    let mut renderer = Renderer::new();
    renderer.prepare_frame(&model, area);
    wait_for_syntax_ready(&mut renderer, &model);
    let maximum = renderer
        .diff_viewport_metrics(mode, diff_area, usize::MAX)
        .maximum_vertical_scroll;
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut states = Vec::new();

    let middle = if mode == DiffViewMode::Hunk {
        maximum.saturating_sub(3)
    } else {
        maximum / 2
    };
    for scroll in [0, middle, maximum] {
        model.diff_scroll = scroll;
        renderer.prepare_frame(&model, area);
        terminal
            .draw(|frame| renderer.render(frame, &model))
            .unwrap();

        let viewport = renderer.diff_viewport_metrics(mode, diff_area, scroll);
        assert!(viewport.horizontal_area.is_empty());
        let rail = renderer.scrollbars.vertical_area;
        let marker_column = rail.x.saturating_add(1);
        let changes = match mode {
            DiffViewMode::Inline => &renderer.highlighted.as_ref().unwrap().inline_changes,
            DiffViewMode::SideBySide => {
                &renderer.highlighted.as_ref().unwrap().side_by_side_changes
            }
            DiffViewMode::Hunk => &renderer.highlighted.as_ref().unwrap().hunk_changes,
        };
        let marker_cells = changes
            .iter()
            .enumerate()
            .map(|(index, change)| {
                let row =
                    rail.y + overview_position(change.first, renderer.scrollbars.rows, rail.height);
                assert_eq!(
                    renderer.change_at_marker(marker_column, row),
                    Some(change.first.saturating_sub(usize::from(index > 0)))
                );
                (
                    row,
                    terminal.backend().buffer()[(marker_column, row)]
                        .symbol()
                        .to_owned(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            renderer.scrollbar_at(rail.x, rail.y),
            Some(crate::diff::ScrollbarAxis::Vertical)
        );
        states.push((
            viewport.content_area,
            rail,
            marker_cells,
            (
                renderer.change_warnings.previous.is_some(),
                renderer.change_warnings.next.is_some(),
            ),
        ));
    }

    assert_eq!(states[0].3, (false, true));
    if mode == DiffViewMode::Hunk {
        assert_ne!(states[1].3, (false, false));
    } else {
        assert_eq!(states[1].3, (true, true));
    }
    assert_eq!(states[2].3, (true, false));
    assert_eq!(states[0].0, states[1].0);
    assert_eq!(states[1].0, states[2].0);
    assert_eq!(states[0].1, states[1].1);
    assert_eq!(states[1].1, states[2].1);
    assert_eq!(states[0].2, states[1].2);
    assert_eq!(states[1].2, states[2].2);
}

#[test]
fn diff_content_uses_the_full_inner_area_in_narrow_layouts() {
    let mut model = model();
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = navigation_layout_patch();
    let mut renderer = Renderer::new();
    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
    wait_for_syntax_ready(&mut renderer, &model);

    for mode in [
        DiffViewMode::Inline,
        DiffViewMode::SideBySide,
        DiffViewMode::Hunk,
    ] {
        for height in 0..=4 {
            let area = Rect::new(7, 9, 20, height);
            let viewport = renderer.diff_viewport_metrics_at(mode, area, 0);
            assert_eq!(viewport.content_area, diff_panel_inner(area));
        }
    }
}

#[test]
fn change_warnings_share_one_row_only_in_a_one_row_viewport() {
    for mode in [
        DiffViewMode::Inline,
        DiffViewMode::SideBySide,
        DiffViewMode::Hunk,
    ] {
        let mut model = model();
        model.diff_view_mode = mode;
        model.snapshot.files[0].unstaged.as_mut().unwrap().text = navigation_layout_patch();
        let mut renderer = Renderer::new();
        renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));
        wait_for_syntax_ready(&mut renderer, &model);

        for (height, expected_rows) in [(2, 0), (3, 1), (4, 2)] {
            let area = Rect::new(0, 0, 20, height);
            let scroll = if mode == DiffViewMode::Hunk { 8 } else { 100 };
            let viewport = renderer.diff_viewport_metrics_at(mode, area, scroll);
            let backend = TestBackend::new(area.width, area.height);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|frame| renderer.render_change_warnings(frame, &viewport))
                .unwrap();

            let warning_rows = [
                renderer.change_warnings.previous,
                renderer.change_warnings.next,
            ]
            .into_iter()
            .flatten()
            .map(|area| area.y)
            .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                warning_rows.len(),
                expected_rows,
                "mode={mode:?}, height={height}, viewport={viewport:?}"
            );
        }
    }
}

fn navigation_layout_patch() -> String {
    let mut patch = String::from("@@ -1,240 +1,240 @@\n");
    for line in 1..=240 {
        if matches!(line, 3 | 238) {
            writeln!(patch, "-old {line}").unwrap();
            writeln!(patch, "+new {line}").unwrap();
        } else {
            writeln!(patch, " line {line:02}").unwrap();
        }
    }
    patch
}
