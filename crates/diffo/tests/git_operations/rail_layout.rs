use super::support::*;

#[test]
fn mock_change_warnings_toggle_without_moving_their_rows() -> Result<()> {
    let repository = TestRepository::new()?;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../diffo-core/fixtures/repository-state.ron")
        .canonicalize()
        .context("resolve mock repository fixture")?;
    let trace_path = repository.root.path().join("mock-rail-frames.ronl");
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[
            ("DIFFO_MOCK_FILE", fixture.as_os_str()),
            ("DIFFO_TRACE_FRAMES", trace_path.as_os_str()),
        ],
    )?;
    let previous = Selector::text(" Previous change (p)");
    let next = Selector::text(" Next change (n)");

    screen
        .press_many(Key::Char('k'), 2)?
        .wait_for_text("new first change")?
        .wait_for(&next)?
        .wait_for_text_gone(" Previous change (p)")?;
    let next_row = screen.position(&next)?.context("next warning row")?.1;

    screen
        .press_many(Key::Down, 2)?
        .wait_for_text_gone("new first change")?
        .wait_for(&previous)?
        .wait_for(&next)?;
    let middle_previous_row = screen
        .position(&previous)?
        .context("middle previous warning row")?
        .1;
    assert_eq!(
        screen.position(&next)?.context("middle next warning")?.1,
        next_row
    );

    screen
        .drag_vertical_scrollbar(0, 100)?
        .wait_for_text("new last change")?
        .wait_for_text_gone(" Next change (n)")?
        .wait_for(&previous)?;
    assert_eq!(
        screen
            .position(&previous)?
            .context("ending previous warning")?
            .1,
        middle_previous_row
    );
    screen.press(Key::Char('q'))?.wait_for_exit()?;
    drop(screen);

    let trace = fs::read_to_string(&trace_path).context("read mock rail frame trace")?;
    let frames = trace
        .lines()
        .map(ron::from_str::<BufferFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mock_frames = frames
        .iter()
        .filter(|frame| frame.displayed_diff.as_deref() == Some("Unstaged:src/rail-layout.rs"))
        .collect::<Vec<_>>();
    assert!(
        mock_frames.len() >= 3,
        "trace did not cover mock scrolling:\n{trace}"
    );
    assert!(
        mock_frames.iter().all(|frame| {
            frame.syntax_ready && frame.first_rendered_row == frame.scroll_after.0
        })
    );
    assert!(mock_frames.iter().any(|frame| {
        frame
            .input_events
            .iter()
            .any(|event| event.contains("code: Down") || event.contains("Scroll"))
    }));
    Ok(())
}
