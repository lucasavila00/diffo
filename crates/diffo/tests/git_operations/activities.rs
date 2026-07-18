use super::support::*;

#[test]
fn tab_cycles_activities_and_restores_diff_overlay_state() -> Result<()> {
    let repository = changed_repository()?;
    let mut screen = repository.screen()?;

    screen
        .wait_for_text("Changes")?
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("pull")?
        .press(Key::Tab)?
        .wait_for_text("Explorer")?
        .wait_for_text_gone("Command Palette")?;
    let contents = screen.contents();
    assert!(!contents.contains("Changes"), "{contents}");

    screen
        .press(Key::Tab)?
        .press(Key::Tab)?
        .wait_for_text("Command Palette")?
        .wait_for_text("pull")?;
    Ok(())
}

#[test]
fn activity_bar_clicks_select_tools_and_diff_returns_intact() -> Result<()> {
    let repository = changed_repository()?;
    let mut screen = repository.screen()?;

    screen
        .wait_for_text("Changes")?
        .click(&Selector::text("⌕"))?
        .wait_for_text_gone("Changes")?
        .click(&Selector::text("≠"))?
        .wait_for_text("Changes")?;
    Ok(())
}

#[test]
fn empty_activity_keeps_quit_available() -> Result<()> {
    changed_repository()?
        .screen()?
        .press(Key::Tab)?
        .press(Key::Char('q'))?
        .wait_for_exit()?;
    Ok(())
}

#[test]
fn delayed_explorer_open_commits_only_the_latest_syntax_ready_file() -> Result<()> {
    let repository = TestRepository::new()?;
    std::fs::write(
        repository.worktree.join("a.rs"),
        "pub const EXPLORER_ALPHA: usize = 1;\n",
    )?;
    std::fs::write(
        repository.worktree.join("b.rs"),
        "pub const EXPLORER_BRAVO: usize = 2;\n",
    )?;
    git(&repository.worktree, &["add", "a.rs", "b.rs"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Add Explorer files"],
    )?;
    let mut screen = repository.screen_with_explorer_delay()?;

    screen
        .press(Key::Tab)?
        .wait_for_text("a.rs")?
        .press(Key::Char('j'))?
        .wait_for_text("EXPLORER_BRAVO")?;

    assert!(!screen.contents().contains("EXPLORER_ALPHA"));
    Ok(())
}
