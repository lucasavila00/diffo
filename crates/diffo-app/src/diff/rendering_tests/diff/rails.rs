use super::*;

#[test]
fn change_button_availability_does_not_move_the_diff_rails() {
    for mode in [DiffViewMode::Inline, DiffViewMode::SideBySide] {
        assert_stable_diff_rails(mode);
    }
}

fn assert_stable_diff_rails(mode: DiffViewMode) {
    let mut model = model();
    model.diff_view_mode = mode;
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = navigation_layout_patch();
    let area = Rect::new(0, 0, 100, 30);
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

    for scroll in [0, 10, maximum] {
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
        };
        let marker_cells = changes
            .iter()
            .map(|change| {
                let row =
                    rail.y + overview_position(change.first, renderer.scrollbars.rows, rail.height);
                assert_eq!(
                    renderer.change_at_marker(marker_column, row, &model),
                    Some(change.first)
                );
                (
                    row,
                    terminal.backend().buffer()[(marker_column, row)]
                        .symbol()
                        .to_owned(),
                )
            })
            .collect::<Vec<_>>();
        if viewport.previous_change.is_none() {
            assert_eq!(
                renderer.hunk_button_direction_at(
                    viewport.content_area.x,
                    viewport.content_area.y.saturating_sub(1),
                ),
                None
            );
        }
        if viewport.next_change.is_none() {
            assert_eq!(
                renderer.hunk_button_direction_at(
                    viewport.content_area.x,
                    viewport.content_area.bottom(),
                ),
                None
            );
        }
        assert_eq!(
            renderer.scrollbar_at(rail.x, rail.y),
            Some(crate::diff::ScrollbarAxis::Vertical)
        );
        states.push((
            viewport.content_area,
            rail,
            marker_cells,
            (
                renderer.hunk_buttons.previous.is_some(),
                renderer.hunk_buttons.next.is_some(),
            ),
        ));
    }

    assert_eq!(states[0].3, (false, true));
    assert_eq!(states[1].3, (true, true));
    assert_eq!(states[2].3, (true, false));
    assert_eq!(states[0].0, states[1].0);
    assert_eq!(states[1].0, states[2].0);
    assert_eq!(states[0].1, states[1].1);
    assert_eq!(states[1].1, states[2].1);
    assert_eq!(states[0].2, states[1].2);
    assert_eq!(states[1].2, states[2].2);
}

#[test]
fn fixed_change_button_lanes_saturate_in_narrow_layouts() {
    let mut model = model();
    model.snapshot.files[0].unstaged.as_mut().unwrap().text = navigation_layout_patch();
    let mut renderer = Renderer::new();
    renderer.prepare_frame(&model, Rect::new(0, 0, 100, 30));

    for mode in [DiffViewMode::Inline, DiffViewMode::SideBySide] {
        for height in 0..=4 {
            let area = Rect::new(7, 9, 20, height);
            let viewport = renderer.diff_viewport_metrics_at(mode, area, 0);
            assert!(viewport.content_area.y >= area.y);
            assert!(viewport.content_area.bottom() <= area.bottom());
            assert!(viewport.content_area.x >= area.x);
            assert!(viewport.content_area.right() <= area.right());
        }
    }
}

fn navigation_layout_patch() -> String {
    let mut patch = String::from("@@ -1,40 +1,40 @@\n");
    for line in 1..=40 {
        if matches!(line, 3 | 38) {
            writeln!(patch, "-old {line}").unwrap();
            writeln!(patch, "+new {line}").unwrap();
        } else {
            writeln!(patch, " line {line:02}").unwrap();
        }
    }
    patch
}
