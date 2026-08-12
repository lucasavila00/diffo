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
        .wait_for(&Selector::file_action("Staged", "00-staged.txt", ""))?
        .wait_for(&Selector::file_action("Changes", "00-change.txt", ""))?
        .scroll_many_at(
            &Selector::file_action("Staged", "00-staged.txt", ""),
            ScrollDirection::Down,
            4,
        )?
        .wait_for(&Selector::file_action("Staged", "10-staged.txt", ""))?
        .wait_for(&Selector::file_action("Changes", "00-change.txt", ""))?;

    screen
        .scroll_many_at(&Selector::text("Changes"), ScrollDirection::Down, 4)?
        .wait_for(&Selector::file_action("Changes", "09-change.txt", ""))?
        .scroll_many_at(&Selector::text("Changes"), ScrollDirection::Up, 4)?
        .wait_for(&Selector::file_action("Changes", "00-change.txt", ""))?;
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
    assert!(panned.contains("Inline ─── "), "{panned}");
    assert!(panned.contains("[ Commands (1 / F1) ]"), "{panned}");

    screen.press_many(Key::Left, 20)?.wait_for_text("START_")?;
    Ok(())
}

#[test]
fn trackpad_horizontal_scroll_is_terminal_safe_and_reversible() -> Result<()> {
    let repository = TestRepository::new()?;
    let line = format!("START_{}\x1b[2JCONTROL_RIGHT_EDGE\n", "x".repeat(40));
    fs::write(repository.worktree.join("tracked.txt"), line)?;
    let mut screen = repository.screen()?;

    screen
        .wait_for_text("START_")?
        .scroll_many(ScrollDirection::Right, 30)?
        .wait_for_text("␛[2JCONTROL_RIGHT_EDGE")?;
    let panned = screen.contents();
    assert!(panned.contains("Inline ─── "), "{panned}");
    assert!(panned.contains("[ Commands (1 / F1) ]"), "{panned}");

    screen
        .scroll_many(ScrollDirection::Left, 30)?
        .wait_for_text("START_")?;
    Ok(())
}

#[test]
fn trackpad_horizontal_scroll_pans_side_by_side_columns() -> Result<()> {
    let repository = TestRepository::new()?;
    let line = format!("START_{}\x1b[2JCONTROL_RIGHT_EDGE\n", "x".repeat(20));
    fs::write(repository.worktree.join("tracked.txt"), line)?;
    let mut screen = repository.screen()?;

    screen
        .wait_for_text("START_")?
        .press(Key::Char('r'))?
        .wait_for_text("Side by side")?
        .scroll_many(ScrollDirection::Right, 30)?
        .wait_for_text("␛[2JCONTROL_RIGHT_EDGE")?
        .scroll_many(ScrollDirection::Left, 30)?
        .wait_for_text("START_")?
        .drag_horizontal_scrollbar(0, 100)?
        .wait_for_text("␛[2JCONTROL_RIGHT_EDGE")?
        .drag_horizontal_scrollbar(100, 0)?
        .wait_for_text("START_")?;
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
fn fully_visible_changes_are_skipped_in_both_directions_without_wrapping() -> Result<()> {
    let repository = TestRepository::new()?;
    let path = repository.worktree.join("clustered-navigation.rs");
    install_navigation_fixture(
        &repository,
        &path,
        clustered_navigation_file(false)?,
        clustered_navigation_file(true)?,
    )?;

    let mut screen = repository.screen()?;
    screen
        .wait_for_text("EARLY_CLUSTER_CHANGE")?
        .wait_for_text_gone(" Previous change (p)")?
        .press(Key::Char('n'))?
        .wait_for_text("CLUSTER_CHANGE_A")?
        .wait_for_text("CLUSTER_CHANGE_B")?
        .wait_for_text("CLUSTER_CHANGE_C")?
        .press(Key::Char('p'))?
        .wait_for_text("EARLY_CLUSTER_CHANGE")?;

    screen.press(Key::Char('p'))?;
    thread::sleep(Duration::from_millis(100));
    screen
        .wait_for_text("EARLY_CLUSTER_CHANGE")?
        .press(Key::Char('n'))?
        .wait_for_text("CLUSTER_CHANGE_A")?
        .press(Key::Char('n'))?
        .wait_for_text("LATE_CLUSTER_CHANGE")?
        .wait_for_text_gone(" Next change (n)")?
        .press(Key::Char('n'))?;
    thread::sleep(Duration::from_millis(100));
    screen
        .wait_for_text("LATE_CLUSTER_CHANGE")?
        .press(Key::Char('p'))?
        .wait_for_text("CLUSTER_CHANGE_C")?;
    Ok(())
}

#[test]
fn viewport_spanning_change_is_one_atomic_navigation_stop() -> Result<()> {
    let repository = TestRepository::new()?;
    let path = repository.worktree.join("tall-change.rs");
    install_navigation_fixture(
        &repository,
        &path,
        tall_change_file(false)?,
        tall_change_file(true)?,
    )?;
    let trace_path = repository
        .root
        .path()
        .join("whole-block-navigation-frames.ronl");
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("DIFFO_TRACE_FRAMES", trace_path.as_os_str())],
    )?;

    screen
        .wait_for_text("EARLY_BLOCK_CHANGE")?
        .press(Key::Char('n'))?
        .wait_for_text("OLD_TALL_CHANGE_050")?
        .press(Key::Char('n'))?
        .wait_for_text("LATE_BLOCK_CHANGE")?
        .wait_for_text_gone(" Next change (n)")?
        .press(Key::Char('p'))?
        .wait_for_text("OLD_TALL_CHANGE_050")?
        .press(Key::Char('p'))?
        .wait_for_text("EARLY_BLOCK_CHANGE")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;
    drop(screen);

    let trace =
        fs::read_to_string(&trace_path).context("read whole-block navigation frame trace")?;
    let frames = trace
        .lines()
        .map(ron::from_str::<BufferFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let input_index = frames
        .iter()
        .enumerate()
        .filter(|(_, frame)| {
            frame
                .input_events
                .iter()
                .any(|event| event.contains("Char('n')"))
        })
        .nth(1)
        .map(|(index, _)| index)
        .with_context(|| format!("trace has no next-change input frame:\n{trace}"))?;
    let old_scroll = frames[input_index].scroll_before.0;
    let commit_offset = frames[input_index..]
        .iter()
        .position(|frame| frame.viewport_transition.is_some())
        .with_context(|| format!("trace has no whole-block navigation commit:\n{trace}"))?;
    let commit_index = input_index + commit_offset;
    assert!(frames[input_index..commit_index].iter().all(|frame| {
        frame.viewport_transition.is_none() && frame.first_rendered_row == old_scroll
    }));
    let committed = &frames[commit_index];
    let target = committed
        .viewport_transition
        .context("whole-block navigation target")?
        .0;
    assert!(
        target >= old_scroll.saturating_add(160),
        "navigation stopped inside the viewport-spanning block: {old_scroll} -> {target}"
    );
    assert!(committed.syntax_ready);
    assert_eq!(committed.first_rendered_row, target);
    assert_eq!(committed.scroll_after.0, target);
    Ok(())
}

#[test]
fn inline_and_side_by_side_each_treat_a_replacement_as_one_block() -> Result<()> {
    let repository = TestRepository::new()?;
    let path = repository.worktree.join("projection-navigation.rs");
    install_navigation_fixture(
        &repository,
        &path,
        projection_navigation_file(false)?,
        projection_navigation_file(true)?,
    )?;

    let mut screen = repository.screen()?;
    screen
        .wait_for_text("OLD_PROJECTION_CHANGE_00")?
        .wait_for_text_gone(" Next change (n)")?
        .wait_for_text_gone(" Previous change (p)")?
        .press(Key::Char('r'))?
        .wait_for_text("Side by side")?
        .wait_for_text("NEW_PROJECTION_CHANGE_00")?
        .wait_for_text_gone(" Next change (n)")?
        .wait_for_text_gone(" Previous change (p)")?;
    Ok(())
}

#[test]
fn wheel_momentum_cannot_displace_an_atomic_next_change_target() -> Result<()> {
    let repository = TestRepository::new()?;
    let path = repository.worktree.join("delayed-navigation.rs");
    fs::write(&path, delayed_navigation_file(false)?)?;
    git(&repository.worktree, &["add", "delayed-navigation.rs"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Add delayed navigation fixture"],
    )?;
    fs::write(&path, delayed_navigation_file(true)?)?;
    let trace_path = repository.root.path().join("atomic-navigation-frames.ronl");
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("DIFFO_TRACE_FRAMES", trace_path.as_os_str())],
    )?;
    screen
        .wait_for_text("FIRST_DELAYED_CHANGE")?
        .scroll_key_scroll(ScrollDirection::Down, 2, Key::Char('n'), 3)?
        .wait_for_text("MIDDLE_DELAYED_CHANGE")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;
    drop(screen);

    let trace = fs::read_to_string(&trace_path).context("read atomic navigation frame trace")?;
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
                .any(|event| event.contains("Char('n')"))
        })
        .with_context(|| format!("trace has no next-change input frame:\n{trace}"))?;
    let input = &frames[input_index];
    assert_eq!(
        input
            .input_events
            .iter()
            .filter(|event| event.contains("ScrollDown"))
            .count(),
        5,
        "trace did not retain the complete wheel tail:\n{trace}"
    );
    assert!(frames.iter().all(|frame| {
        !frame
            .input_events
            .iter()
            .any(|event| event.contains("ScrollDown"))
            || frame
                .input_events
                .iter()
                .any(|event| event.contains("Char('n')"))
    }));
    let old_scroll = input.scroll_before.0;
    assert_eq!(input.viewport_transition, None);
    assert_eq!(input.scroll_after.0, old_scroll);
    assert_eq!(input.first_rendered_row, old_scroll);
    let commit_offset = frames[input_index + 1..]
        .iter()
        .position(|frame| frame.viewport_transition.is_some())
        .with_context(|| format!("trace has no navigation commit:\n{trace}"))?;
    let commit_index = input_index + 1 + commit_offset;
    assert!(frames[input_index..commit_index].iter().all(|frame| {
        frame.viewport_transition.is_none() && frame.first_rendered_row == old_scroll
    }));
    let committed = &frames[commit_index];
    let target = committed
        .viewport_transition
        .context("navigation target")?
        .0;
    assert!(target > 300, "unexpected navigation target: {target}");
    assert!(committed.syntax_ready);
    assert_eq!(committed.first_rendered_row, target);
    assert_eq!(committed.scroll_after.0, target);
    assert!(
        frames[commit_index..]
            .iter()
            .all(|frame| { frame.first_rendered_row == target && frame.scroll_after.0 == target })
    );
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
        diffo_binary()?,
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

#[test]
fn cold_scrolls_cross_coverage_in_both_directions_without_empty_frames() -> Result<()> {
    let repository = TestRepository::new()?;
    let path = repository.worktree.join("middle-syntax.rs");
    fs::write(&path, middle_syntax_file(false)?)?;
    git(&repository.worktree, &["add", "middle-syntax.rs"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Add middle syntax fixture"],
    )?;
    fs::write(&path, middle_syntax_file(true)?)?;
    let trace_path = repository.root.path().join("direction-neutral-frames.ronl");
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("DIFFO_TRACE_FRAMES", trace_path.as_os_str())],
    )?;
    screen
        .wait_for_text("MIDDLE_CHANGE")?
        .press_many(Key::PageUp, 6)?
        .wait_for_text("LINE_0200")?
        .press_many(Key::PageDown, 12)?
        .wait_for_text("LINE_0525")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;
    drop(screen);

    let trace = fs::read_to_string(&trace_path).context("read direction-neutral frame trace")?;
    let frames = trace
        .lines()
        .map(ron::from_str::<BufferFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for key in ["PageUp", "PageDown"] {
        let input = frames
            .iter()
            .position(|frame| frame.input_events.iter().any(|event| event.contains(key)))
            .with_context(|| format!("trace has no {key} input frame:\n{trace}"))?;
        let frame = &frames[input];
        assert!(
            frame.syntax_ready,
            "{key} exposed an empty syntax-skeleton frame:\n{trace}"
        );
        assert_eq!(
            frame.scroll_after, frame.scroll_before,
            "{key} moved before its cold syntax target was ready:\n{trace}"
        );
        let committed = frames[input + 1..]
            .iter()
            .find_map(|frame| frame.viewport_transition.map(|viewport| viewport.0))
            .with_context(|| format!("{key} target never committed:\n{trace}"))?;
        if key == "PageUp" {
            assert!(
                committed < frame.scroll_before.0,
                "{key} target did not move upward:\n{trace}"
            );
        } else {
            assert!(
                committed > frame.scroll_before.0,
                "{key} target did not move downward:\n{trace}"
            );
        }
        assert!(
            committed.abs_diff(frame.scroll_before.0) > 100,
            "{key} did not cross the initial retained coverage boundary:\n{trace}"
        );
    }
    Ok(())
}

fn middle_syntax_file(changed: bool) -> Result<String> {
    let mut contents = String::new();
    for line in 1..=700 {
        if changed && line == 350 {
            writeln!(contents, "pub const MIDDLE_CHANGE: usize = 0;")
                .context("build middle syntax target")?;
        } else {
            writeln!(contents, "pub const LINE_{line:04}: usize = {line};")
                .context("build middle syntax fixture")?;
        }
    }
    Ok(contents)
}

fn delayed_navigation_file(changed: bool) -> Result<String> {
    let mut contents = String::new();
    for line in 0..700 {
        let value = match (changed, line) {
            (true, 10) => "FIRST_DELAYED_CHANGE".to_owned(),
            (true, 350) => "MIDDLE_DELAYED_CHANGE".to_owned(),
            (true, 690) => "LAST_DELAYED_CHANGE".to_owned(),
            _ => format!("value_{line:03}"),
        };
        writeln!(contents, "pub const DELAYED_{line:03}: &str = \"{value}\";")
            .context("build delayed navigation file")?;
    }
    Ok(contents)
}

fn install_navigation_fixture(
    repository: &TestRepository,
    path: &Path,
    before: String,
    after: String,
) -> Result<()> {
    fs::write(path, before)?;
    let relative = path
        .strip_prefix(&repository.worktree)
        .context("navigation fixture is outside the worktree")?;
    git(
        &repository.worktree,
        &["add", relative.to_string_lossy().as_ref()],
    )?;
    git(
        &repository.worktree,
        &["commit", "-m", "Add viewport navigation fixture"],
    )?;
    fs::write(path, after)?;
    Ok(())
}

fn clustered_navigation_file(changed: bool) -> Result<String> {
    let mut contents = String::new();
    for line in 0..130 {
        let value = match (changed, line) {
            (true, 10) => "EARLY_CLUSTER_CHANGE".to_owned(),
            (true, 50) => "CLUSTER_CHANGE_A".to_owned(),
            (true, 55) => "CLUSTER_CHANGE_B".to_owned(),
            (true, 60) => "CLUSTER_CHANGE_C".to_owned(),
            (true, 110) => "LATE_CLUSTER_CHANGE".to_owned(),
            _ => format!("cluster_value_{line:03}"),
        };
        writeln!(contents, "pub const CLUSTER_{line:03}: &str = \"{value}\";")
            .context("build clustered navigation file")?;
    }
    Ok(contents)
}

fn tall_change_file(changed: bool) -> Result<String> {
    let mut contents = String::new();
    for line in 0..220 {
        let value = match (changed, line) {
            (true, 5) => "EARLY_BLOCK_CHANGE".to_owned(),
            (true, 50..=129) => format!("NEW_TALL_CHANGE_{line:03}"),
            (true, 150) => "LATE_BLOCK_CHANGE".to_owned(),
            _ => format!("OLD_TALL_CHANGE_{line:03}"),
        };
        writeln!(contents, "{value}").context("build tall change file")?;
    }
    Ok(contents)
}

fn projection_navigation_file(changed: bool) -> Result<String> {
    let mut contents = String::new();
    for line in 0..40 {
        let prefix = if changed && line < 15 { "NEW" } else { "OLD" };
        writeln!(contents, "{prefix}_PROJECTION_CHANGE_{line:02}")
            .context("build projection navigation file")?;
    }
    Ok(contents)
}
