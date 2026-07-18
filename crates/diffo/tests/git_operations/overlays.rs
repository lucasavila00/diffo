use super::support::*;

#[test]
fn clicking_palette_result_runs_command() -> Result<()> {
    let repository = TestRepository::new()?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("pull")?
        .click(&Selector::text("Git: Pull"))?;

    wait_for("clicked pull command to finish", || {
        Ok(repository.worktree.join("remote.txt").exists())
    })
}

#[test]
fn overlays_open_and_close_with_function_keys() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Function(1))?
        .wait_for_text("Command Palette")?
        .press(Key::Escape)?
        .wait_for_text_gone("Command Palette")?
        .press(Key::Function(2))?
        .wait_for_text("Help")?
        .wait_for_text("Shortcut")?
        .wait_for_text("Action")?
        .wait_for_text("Next activity")?
        .wait_for_text("k / l / s")?
        .wait_for_text("Next file")?
        .wait_for_text("Page Up")?
        .wait_for_text("Scroll up one page")?
        .wait_for_text("Stage / unstage selected file")?
        .press(Key::Function(2))?
        .wait_for_text_gone("Help")?
        .press(Key::Char('2'))?
        .wait_for_text("Help")?
        .press(Key::Char('2'))?
        .wait_for_text_gone("Help")?;
    Ok(())
}
