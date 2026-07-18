use super::support::*;

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
        .click(&Selector::text("↓ Next change"))?
        .wait_for_text("MIDDLE_CHANGE")?;
    assert!(screen.contents().contains("↑ Previous change"));
    screen
        .click(&Selector::text("↓ Next change"))?
        .wait_for_text("LAST_CHANGE")?
        .wait_for_text_gone("↓ Next change")?
        .click(&Selector::text("↑ Previous change"))?
        .wait_for_text("MIDDLE_CHANGE")?;
    Ok(())
}
