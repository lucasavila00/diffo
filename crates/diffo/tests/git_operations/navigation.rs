use super::support::*;

#[test]
fn mouse_click_selects_a_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(
        repository.worktree.join("tracked.txt"),
        "tracked selected\n",
    )?;
    fs::write(repository.worktree.join("new.txt"), "new selected\n")?;
    let mut screen = repository.screen()?;

    screen
        .click(&Selector::text("new.txt"))?
        .wait_for(&Selector::selected_row("new.txt"))?
        .wait_for_text("new selected")?;
    Ok(())
}

#[test]
fn view_toggle_renders_immediately() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('r'))?
        .wait_for_text("Side by side")?;
    assert!(screen.contents().contains("Side by side ─── "));
    Ok(())
}

#[test]
fn full_screen_diff_shows_raw_hunks_and_exits_by_key_or_x() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('f'))?
        .wait_for_text_gone("Changes")?
        .wait_for_text("@@ -1 +1 @@")?;
    let contents = screen.contents();
    assert!(contents.contains("-base"));
    assert!(contents.contains("+changed"));
    assert!(!contents.contains("File Diff"));
    assert!(!contents.contains("branch master"));

    screen
        .press(Key::Char('F'))?
        .wait_for_text("@@ -1 +1 @@")?
        .press(Key::Char('f'))?
        .wait_for_text("Changes")?
        .press(Key::Char('f'))?
        .wait_for_text_gone("Changes")?
        .click(&Selector::text("X"))?
        .wait_for_text("Changes")?;
    Ok(())
}

#[test]
fn full_screen_explorer_shows_only_the_open_file_and_scroll_controls() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Tab)?
        .wait_for_text("Explorer")?
        .wait_for_text("base")?
        .click(&Selector::text(""))?
        .wait_for_text_gone("Explorer")?
        .wait_for_text("base")?;
    let contents = screen.contents();
    assert!(contents.contains("tracked.txt"));
    assert!(!contents.contains("previous"));
    assert!(!contents.contains("branch master"));

    screen.press(Key::Char('f'))?.wait_for_text("Explorer")?;
    Ok(())
}

#[test]
fn every_file_navigation_alias_moves_selection() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    fs::write(repository.worktree.join("new.txt"), "new\n")?;
    let mut screen = repository.screen()?;
    screen.wait_for(&Selector::selected_row("tracked.txt"))?;

    screen
        .press(Key::End)?
        .wait_for(&Selector::selected_row("new.txt"))?
        .press(Key::Char('g'))?
        .wait_for(&Selector::selected_row("tracked.txt"))?
        .press(Key::Char('k'))?
        .wait_for(&Selector::selected_row("new.txt"))?
        .press(Key::Home)?
        .wait_for(&Selector::selected_row("tracked.txt"))?
        .press(Key::Char('s'))?
        .wait_for(&Selector::selected_row("new.txt"))?
        .press(Key::Char('w'))?
        .wait_for(&Selector::selected_row("tracked.txt"))?
        .press(Key::Char('k'))?
        .wait_for(&Selector::selected_row("new.txt"))?
        .press(Key::Char('j'))?
        .wait_for(&Selector::selected_row("tracked.txt"))?
        .press(Key::Char('l'))?
        .wait_for(&Selector::selected_row("new.txt"))?
        .press(Key::Char('j'))?
        .wait_for(&Selector::selected_row("tracked.txt"))?;
    Ok(())
}

#[test]
fn q_and_control_c_exit_cleanly() -> Result<()> {
    let repository = TestRepository::new()?;
    repository
        .screen()?
        .press(Key::Char('q'))?
        .wait_for_exit()?;
    repository
        .screen()?
        .press(Key::Ctrl('c'))?
        .wait_for_exit()?;
    repository
        .screen()?
        .click(&Selector::text("Type a message"))?
        .press(Key::Ctrl('c'))?
        .wait_for_exit()?;
    Ok(())
}
