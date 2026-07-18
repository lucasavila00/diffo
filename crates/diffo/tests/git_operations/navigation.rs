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
fn view_and_file_pane_toggles_render_immediately() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('r'))?
        .wait_for_text("Side by side")?
        .press(Key::Char('e'))?
        .wait_for_text_gone("Changes")?;
    assert!(screen.contents().contains("File Diff"));
    screen.press(Key::Char('e'))?.wait_for_text("Changes")?;
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
        .press(Key::Home)?
        .wait_for(&Selector::selected_row("tracked.txt"))?
        .press(Key::Char('G'))?
        .wait_for(&Selector::selected_row("new.txt"))?
        .press(Key::Char('g'))?
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
