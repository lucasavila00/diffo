use super::support::*;

#[test]
fn delayed_diff_open_commits_only_the_latest_buffer_at_its_first_change() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("a-small.txt"), "small base\n")?;
    fs::write(
        repository.worktree.join("b-large.txt"),
        large_file("b base", None)?,
    )?;
    fs::write(
        repository.worktree.join("c-large.txt"),
        large_file("c base", None)?,
    )?;
    git(&repository.worktree, &["add", "."])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Add navigation fixtures"],
    )?;
    fs::write(repository.worktree.join("a-small.txt"), "SMALL_CHANGED\n")?;
    fs::write(
        repository.worktree.join("b-large.txt"),
        large_file("b base", Some("B_LARGE_CHANGED"))?,
    )?;
    fs::write(
        repository.worktree.join("c-large.txt"),
        large_file("c base", Some("C_LARGE_CHANGED"))?,
    )?;
    let trace_path = repository.root.path().join("atomic-open-frames.ronl");
    let mut screen = DiffoScreen::launch_with_env(
        env!("CARGO_BIN_EXE_diffo"),
        &repository.worktree,
        &[
            ("DIFFO_TRACE_FRAMES", trace_path.as_os_str()),
            ("DIFFO_E2E_DIFF_PREP_DELAY_MS", OsStr::new("300")),
        ],
    )?;
    screen
        .wait_for_text("SMALL_CHANGED")?
        .press(Key::Char('s'))?
        .wait_for(&Selector::selected_row("b-large.txt"))?;
    assert!(screen.contents().contains("SMALL_CHANGED"));
    assert!(
        screen
            .contents()
            .lines()
            .next()
            .unwrap_or_default()
            .contains("M a-small.txt")
    );
    screen
        .press(Key::Char('s'))?
        .wait_for(&Selector::selected_row("c-large.txt"))?;
    assert!(screen.contents().contains("SMALL_CHANGED"));
    screen
        .wait_for_text("C_LARGE_CHANGED")?
        .wait_for_text("M c-large.txt")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;
    drop(screen);

    let trace = fs::read_to_string(&trace_path).context("read atomic-open frame trace")?;
    let frames = trace
        .lines()
        .map(ron::from_str::<BufferFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let requested_b = "Unstaged:b-large.txt";
    let requested_c = "Unstaged:c-large.txt";
    let displayed_a = "Unstaged:a-small.txt";
    assert!(frames.iter().any(|frame| {
        frame.requested_diff.as_deref() == Some(requested_b)
            && frame.displayed_diff.as_deref() == Some(displayed_a)
            && frame.viewport_transition.is_none()
    }));
    assert!(frames.iter().any(|frame| {
        frame.requested_diff.as_deref() == Some(requested_c)
            && frame.displayed_diff.as_deref() == Some(displayed_a)
            && frame.viewport_transition.is_none()
    }));
    assert!(
        frames
            .iter()
            .all(|frame| frame.displayed_diff.as_deref() != Some(requested_b)),
        "stale b-large buffer was displayed:\n{trace}"
    );
    let committed = frames
        .iter()
        .find(|frame| {
            frame.displayed_diff.as_deref() == Some(requested_c)
                && frame.viewport_transition.is_some()
        })
        .with_context(|| format!("trace has no atomic c-large commit:\n{trace}"))?;
    let first_change = committed.viewport_transition.context("commit viewport")?.0;
    assert!(
        first_change > 500,
        "unexpected first change row: {first_change}"
    );
    assert_eq!(committed.first_rendered_row, first_change);
    assert!(frames.iter().all(|frame| {
        frame.displayed_diff.as_deref() != Some(requested_c)
            || frame.first_rendered_row == first_change
    }));
    Ok(())
}

#[test]
fn wheel_burst_is_one_bounded_frame_transition() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut contents = String::new();
    for line in 0..200 {
        writeln!(contents, "line {line:03}").context("build trace fixture")?;
    }
    fs::write(repository.worktree.join("tracked.txt"), contents)?;
    let trace_path = repository.root.path().join("frames.ronl");
    let mut screen = DiffoScreen::launch_with_env(
        env!("CARGO_BIN_EXE_diffo"),
        &repository.worktree,
        &[("DIFFO_TRACE_FRAMES", trace_path.as_os_str())],
    )?;
    screen
        .wait_for_text("line 000")?
        .scroll_many(ScrollDirection::Down, 10)?
        .wait_for_text_gone("line 000")?
        .scroll_many(ScrollDirection::Up, 10)?
        .wait_for_text("line 000")?
        .press_many(Key::Down, 10)?
        .wait_for_text_gone("line 000")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;
    drop(screen);

    let trace = fs::read_to_string(&trace_path).context("read frame trace")?;
    let records = trace
        .lines()
        .map(ron::from_str::<ScrollFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let frame = records
        .iter()
        .find(|record| {
            record
                .input_events
                .iter()
                .filter(|event| event.contains("ScrollDown"))
                .count()
                == 10
        })
        .with_context(|| format!("trace has no coalesced wheel frame:\n{trace}"))?;
    assert_eq!(
        frame.scroll_after.0.saturating_sub(frame.scroll_before.0),
        10
    );
    let input_to_draw = frame.draw_end_us.saturating_sub(
        frame
            .event_read_us
            .context("wheel frame has no event time")?,
    );
    assert!(
        input_to_draw < 250_000,
        "input-to-draw took {input_to_draw}µs"
    );
    let key_frame = records
        .iter()
        .find(|record| {
            record
                .input_events
                .iter()
                .filter(|event| event.contains("code: Down"))
                .count()
                == 10
        })
        .with_context(|| format!("trace has no coalesced key-repeat frame:\n{trace}"))?;
    assert_eq!(
        key_frame
            .scroll_after
            .0
            .saturating_sub(key_frame.scroll_before.0),
        40
    );
    Ok(())
}

#[test]
fn live_content_change_keeps_the_visible_line_anchored() -> Result<()> {
    let repository = TestRepository::new()?;
    let contents = numbered_lines(120, false)?;
    fs::write(repository.worktree.join("tracked.txt"), contents)?;
    let mut screen = repository.screen()?;
    screen
        .wait_for_text("line 000")?
        .press_many(Key::Down, 10)?
        .wait_for_text("line 040")?;
    let anchor = Selector::text("line 040");
    let before = screen
        .position(&anchor)?
        .context("anchor line is not visible before refresh")?;

    let mut changed = String::new();
    for index in 0..5 {
        writeln!(changed, "inserted {index}").context("build inserted lines")?;
    }
    changed.push_str(&numbered_lines(120, true)?);
    fs::write(repository.worktree.join("tracked.txt"), changed)?;
    screen.wait_for_text("changed neighbor")?;
    let after = screen
        .position(&anchor)?
        .context("anchor line is not visible after refresh")?;

    assert_eq!(
        after.1, before.1,
        "content refresh moved the visible anchor"
    );
    Ok(())
}
