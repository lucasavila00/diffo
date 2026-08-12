use super::support::*;

#[test]
fn change_navigation_links_and_shortcuts_navigate_atomically() -> Result<()> {
    let repository = TestRepository::new()?;
    let path = repository.worktree.join("navigation.rs");
    fs::write(&path, navigation_file(false)?)?;
    git(&repository.worktree, &["add", "navigation.rs"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Add hunk navigation fixture"],
    )?;
    fs::write(&path, navigation_file(true)?)?;

    let trace_path = repository.root.path().join("change-link-frames.ronl");
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("DIFFO_TRACE_FRAMES", trace_path.as_os_str())],
    )?;
    screen
        .wait_for_text("FIRST_CHANGE")?
        .click(&Selector::text(" Next change (n)"))?
        .wait_for_text("MIDDLE_CHANGE")?;
    assert!(screen.contents().contains(" Previous change (p)"));
    screen
        .press(Key::Char('n'))?
        .wait_for_text("LAST_CHANGE")?
        .wait_for_text_gone(" Next change (n)")?
        .click(&Selector::text(" Previous change (p)"))?
        .wait_for_text("MIDDLE_CHANGE")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;
    drop(screen);

    let trace = fs::read_to_string(&trace_path).context("read change-link frame trace")?;
    let frames = trace
        .lines()
        .map(ron::from_str::<BufferFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let input_index = frames
        .iter()
        .position(|frame| {
            frame
                .input_events
                .iter()
                .any(|event| event.contains("Down(Left)"))
        })
        .with_context(|| format!("trace has no change-link click frame:\n{trace}"))?;
    let old_scroll = frames[input_index].scroll_before.0;
    let commit_offset = frames[input_index..]
        .iter()
        .position(|frame| frame.viewport_transition.is_some())
        .with_context(|| format!("change-link click never committed:\n{trace}"))?;
    let commit_index = input_index + commit_offset;
    assert!(frames[input_index..commit_index].iter().all(|frame| {
        frame.viewport_transition.is_none() && frame.first_rendered_row == old_scroll
    }));
    let committed = &frames[commit_index];
    let target = committed
        .viewport_transition
        .context("change-link viewport transition")?
        .0;
    assert!(target > old_scroll);
    assert!(committed.syntax_ready);
    assert_eq!(committed.first_rendered_row, target);
    assert_eq!(committed.scroll_after.0, target);
    Ok(())
}
