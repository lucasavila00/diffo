use super::support::*;

#[test]
fn terminal_enables_action_mouse_events_without_passive_motion() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut screen = repository.screen()?;
    let output = screen.raw_output();

    assert!(
        output
            .windows(b"\x1b[?1000h\x1b[?1002h".len())
            .any(|window| window == b"\x1b[?1000h\x1b[?1002h"),
        "compiled Diffo did not enable press and drag mouse reporting"
    );
    assert!(
        !output
            .windows(b"\x1b[?1003h".len())
            .any(|window| window == b"\x1b[?1003h"),
        "compiled Diffo enabled passive mouse movement reporting"
    );
    Ok(())
}

#[test]
fn keyboard_and_mouse_scroll_move_the_visible_diff() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut contents = String::new();
    for line in 0..120 {
        writeln!(contents, "line {line:03}").context("build scrolling fixture")?;
    }
    fs::write(repository.worktree.join("tracked.txt"), contents)?;
    let mut screen = repository.screen()?;
    screen.wait_for_text("line 000")?;

    screen
        .press(Key::Down)?
        .wait_for_text_gone("line 000")?
        .press(Key::Up)?
        .wait_for_text("line 000")?
        .press(Key::PageDown)?
        .wait_for_text_gone("line 000")?
        .press(Key::PageUp)?
        .wait_for_text("line 000")?
        .scroll_many(ScrollDirection::Down, 4)?
        .wait_for_text_gone("line 000")?
        .scroll_many(ScrollDirection::Up, 4)?
        .wait_for_text("line 000")?
        .drag_vertical_scrollbar(0, 50)?
        .wait_for_text_gone("line 000")?
        .drag_vertical_scrollbar(50, 0)?
        .wait_for_text("line 000")?;
    Ok(())
}

#[test]
fn mouse_wheel_scrolls_diff_file_panels_independently() -> Result<()> {
    let repository = TestRepository::new()?;
    for index in 0..20 {
        fs::write(
            repository.worktree.join(format!("{index:02}-staged.txt")),
            "staged\n",
        )?;
    }
    git(&repository.worktree, &["add", "."])?;
    for index in 0..20 {
        fs::write(
            repository.worktree.join(format!("{index:02}-change.txt")),
            "change\n",
        )?;
    }
    let mut screen = repository.screen()?;

    screen
        .wait_for(&Selector::file_action("Staged", "00-staged.txt", "[-]"))?
        .wait_for(&Selector::file_action("Changes", "00-change.txt", "[+]"))?
        .scroll_many_at(
            &Selector::file_action("Staged", "00-staged.txt", "[-]"),
            ScrollDirection::Down,
            4,
        )?
        .wait_for(&Selector::file_action("Staged", "10-staged.txt", "[-]"))?
        .wait_for(&Selector::file_action("Changes", "00-change.txt", "[+]"))?;

    screen
        .scroll_many_at(&Selector::text("Changes"), ScrollDirection::Down, 4)?
        .wait_for(&Selector::file_action("Changes", "09-change.txt", "[+]"))?
        .scroll_many_at(&Selector::text("Changes"), ScrollDirection::Up, 4)?
        .wait_for(&Selector::file_action("Changes", "00-change.txt", "[+]"))?;
    Ok(())
}

#[test]
fn mouse_wheel_scrolls_explorer_tree() -> Result<()> {
    let repository = TestRepository::new()?;
    for index in 0..40 {
        fs::write(
            repository.worktree.join(format!("tree-{index:02}.txt")),
            "tree\n",
        )?;
    }
    git(&repository.worktree, &["add", "."])?;
    git(&repository.worktree, &["commit", "-m", "Add tree files"])?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Tab)?
        .wait_for_text("tree-00.txt")?
        .scroll_many_at(&Selector::text("tree-00.txt"), ScrollDirection::Down, 4)?
        .wait_for_text_gone("tree-00.txt")?
        .scroll_many_at(&Selector::text("Explorer"), ScrollDirection::Up, 4)?
        .wait_for_text("tree-00.txt")?;
    Ok(())
}

#[test]
fn horizontal_scrollbar_drags_all_the_way_right() -> Result<()> {
    let repository = TestRepository::new()?;
    let contents = format!("{}RIGHT_EDGE\n", "wide-content-".repeat(80));
    fs::write(repository.worktree.join("tracked.txt"), contents)?;
    let mut screen = repository.screen()?;

    screen
        .wait_for_text("wide-content")?
        .drag_horizontal_scrollbar(0, 100)?
        .wait_for_text("RIGHT_EDGE")?;
    Ok(())
}

#[test]
fn diff_horizontal_pan_is_terminal_safe_and_reversible() -> Result<()> {
    let repository = TestRepository::new()?;
    let line = format!("START_{}\x1b[2JCONTROL_RIGHT_EDGE\n", "x".repeat(100));
    fs::write(repository.worktree.join("tracked.txt"), line)?;
    let mut screen = repository.screen()?;

    screen
        .wait_for_text("START_")?
        .press_many(Key::Right, 20)?
        .wait_for_text("␛[2JCONTROL_RIGHT_EDGE")?;
    let panned = screen.contents();
    assert!(panned.contains("File Diff"), "{panned}");
    assert!(panned.contains("1/f1: commands"), "{panned}");

    screen.press_many(Key::Left, 20)?.wait_for_text("START_")?;
    Ok(())
}

#[test]
fn vertical_scrollbar_reaches_its_end_with_the_last_diff_line() -> Result<()> {
    let repository = TestRepository::new()?;
    let contents = numbered_lines(120, false)?;
    fs::write(repository.worktree.join("tracked.txt"), contents)?;
    let mut screen = repository.screen()?;

    screen
        .wait_for_text("line 000")?
        .drag_vertical_scrollbar(0, 100)?
        .wait_for_text("line 119")?
        .wait_for(&Selector::vertical_scrollbar_end())?;
    Ok(())
}

#[test]
fn large_hunk_buttons_click_between_changes_without_wrapping() -> Result<()> {
    let repository = TestRepository::new()?;
    let path = repository.worktree.join("navigation.rs");
    fs::write(&path, navigation_file(false)?)?;
    git(&repository.worktree, &["add", "navigation.rs"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Add hunk navigation fixture"],
    )?;
    fs::write(&path, navigation_file(true)?)?;

    let mut screen = repository.screen()?;
    screen
        .wait_for_text("FIRST_CHANGE")?
        .click(&Selector::text("↓ Next change (n)"))?
        .wait_for_text("MIDDLE_CHANGE")?;
    assert!(screen.contents().contains("↑ Previous change (p)"));
    screen
        .click(&Selector::text("↓ Next change (n)"))?
        .wait_for_text("LAST_CHANGE")?
        .wait_for_text_gone("↓ Next change (n)")?
        .click(&Selector::text("↑ Previous change (p)"))?
        .wait_for_text("MIDDLE_CHANGE")?;
    Ok(())
}

#[test]
fn n_and_p_move_between_changes_with_the_keyboard() -> Result<()> {
    let repository = TestRepository::new()?;
    let path = repository.worktree.join("keyboard-navigation.rs");
    fs::write(&path, navigation_file(false)?)?;
    git(&repository.worktree, &["add", "keyboard-navigation.rs"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Add keyboard navigation fixture"],
    )?;
    fs::write(&path, navigation_file(true)?)?;

    let mut screen = repository.screen()?;
    screen
        .wait_for_text("FIRST_CHANGE")?
        .press(Key::Char('n'))?
        .wait_for_text("MIDDLE_CHANGE")?
        .press(Key::Char('n'))?
        .wait_for_text("LAST_CHANGE")?
        .press(Key::Char('p'))?
        .wait_for_text("MIDDLE_CHANGE")?
        .press(Key::Char('p'))?
        .wait_for_text("FIRST_CHANGE")?;
    Ok(())
}

#[test]
fn cold_large_file_open_commits_at_a_syntax_ready_first_change() -> Result<()> {
    let repository = TestRepository::new()?;
    let path = repository.worktree.join("large-syntax.rs");
    fs::write(&path, large_syntax_file(false)?)?;
    git(&repository.worktree, &["add", "large-syntax.rs"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Add large syntax fixture"],
    )?;
    fs::write(&path, large_syntax_file(true)?)?;
    let trace_path = repository.root.path().join("large-syntax-frames.ronl");
    let mut screen = DiffoScreen::launch_with_env(
        env!("CARGO_BIN_EXE_diffo"),
        &repository.worktree,
        &[("DIFFO_TRACE_FRAMES", trace_path.as_os_str())],
    )?;
    screen
        .wait_for_text("PERF_TARGET_09000")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;
    drop(screen);

    let trace = fs::read_to_string(&trace_path).context("read large syntax frame trace")?;
    let committed = trace
        .lines()
        .map(ron::from_str::<BufferFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .find(|frame| {
            frame.displayed_diff.as_deref() == Some("Unstaged:large-syntax.rs")
                && frame.viewport_transition.is_some()
        })
        .with_context(|| format!("trace has no large syntax commit:\n{trace}"))?;
    let first_change = committed.viewport_transition.context("commit viewport")?.0;
    assert!(committed.syntax_ready);
    assert_eq!(committed.first_rendered_row, first_change);
    assert!(first_change > 8_900);
    Ok(())
}
