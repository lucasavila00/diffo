use super::support::*;

#[test]
fn command_palette_blocks_activity_switching_and_does_not_restore_hidden_state() -> Result<()> {
    let repository = changed_repository()?;
    let mut screen = repository.screen()?;

    screen
        .wait_for_text("Changes")?
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("pull")?
        .press(Key::Tab)?
        .wait_for_text("Command Palette")?
        .wait_for_text("Changes")?
        .press(Key::Escape)?
        .wait_for_text_gone("Command Palette")?
        .press(Key::Tab)?
        .wait_for_text("Explorer")?
        .wait_for_text_gone("Changes")?;

    screen
        .press(Key::Tab)?
        .press(Key::Tab)?
        .wait_for_text("Changes")?
        .wait_for_text_gone("Command Palette")?;
    Ok(())
}

#[test]
fn activity_palettes_share_git_commands_and_keep_specific_catalogs_separate() -> Result<()> {
    let repository = changed_repository()?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .wait_for_text("Git: Fetch")?
        .wait_for_text_gone("Explorer: Collapse All Folders")?
        .press(Key::Escape)?
        .wait_for_text_gone("Command Palette")?
        .press(Key::Tab)?
        .press(Key::Char('1'))?
        .wait_for_text("Git: Fetch")?
        .wait_for_text("Explorer: Collapse All Folders")?
        .press(Key::Escape)?
        .wait_for_text_gone("Command Palette")?
        .press(Key::Tab)?
        .press(Key::Char('1'))?
        .wait_for_text("Git: Fetch")?
        .wait_for_text_gone("Explorer: Collapse All Folders")?;
    Ok(())
}

#[test]
fn activity_bar_clicks_select_tools_and_diff_returns_intact() -> Result<()> {
    let repository = changed_repository()?;
    let mut screen = repository.screen()?;

    screen
        .wait_for_text("Changes")?
        .click(&Selector::text(""))?
        .wait_for_text_gone("Changes")?
        .click(&Selector::text(""))?
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
fn rapid_explorer_open_commits_only_the_latest_syntax_ready_file() -> Result<()> {
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
    let mut screen = repository.screen()?;

    screen
        .press(Key::Tab)?
        .wait_for_text("a.rs")?
        .press(Key::Char('k'))?
        .wait_for_text("EXPLORER_BRAVO")?;

    assert!(!screen.contents().contains("EXPLORER_ALPHA"));
    assert!(
        screen
            .contents()
            .lines()
            .next()
            .unwrap_or_default()
            .contains("b.rs")
    );
    Ok(())
}

#[test]
fn explorer_removes_a_deleted_file_without_showing_head_content() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("keep.txt"), "KEEP_CONTENT\n")?;
    fs::write(repository.worktree.join("removed.txt"), "REMOVED_CONTENT\n")?;
    git(&repository.worktree, &["add", "keep.txt", "removed.txt"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Add Explorer files"],
    )?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Tab)?
        .wait_for_text("Explorer")?
        .wait_for_text("keep.txt")?
        .click(&Selector::text("keep.txt"))?
        .wait_for_text("KEEP_CONTENT")?
        .click(&Selector::text("removed.txt"))?
        .wait_for_text("REMOVED_CONTENT")?;

    fs::remove_file(repository.worktree.join("removed.txt"))?;

    screen
        .wait_for_text_gone("removed.txt")?
        .wait_for_text("base")?;

    assert!(!screen.contents().contains("REMOVED_CONTENT"));
    Ok(())
}

#[test]
fn explorer_horizontal_pan_is_bounded_and_terminal_safe() -> Result<()> {
    let repository = TestRepository::new()?;
    let line = format!("START_{}\x1b[2JCONTROL_RIGHT_EDGE\n", "x".repeat(100));
    fs::write(repository.worktree.join("tracked.txt"), line)?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Add wide control fixture"],
    )?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Tab)?
        .wait_for_text("START_")?
        .press_many(Key::Right, 20)?
        .wait_for_text("␛[2JCONTROL_RIGHT_EDGE")?;
    let panned = screen.contents();
    assert!(panned.contains("Explorer"), "{panned}");
    assert!(panned.contains("1/f1: commands"), "{panned}");

    screen.press_many(Key::Left, 20)?.wait_for_text("START_")?;
    Ok(())
}
