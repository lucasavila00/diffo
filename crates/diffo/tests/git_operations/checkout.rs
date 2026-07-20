use super::support::*;

#[test]
fn checkout_waits_for_real_git_completion_before_installing_the_new_branch() -> Result<()> {
    let repository = TestRepository::new()?;
    let original = git_output(&repository.worktree, &["branch", "--show-current"])?;
    git(&repository.worktree, &["switch", "-c", "topic"])?;
    fs::write(repository.worktree.join("topic.txt"), "topic\n")?;
    git(&repository.worktree, &["add", "topic.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Topic"])?;
    git(&repository.worktree, &["switch", &original])?;
    let mut gate = diffo_e2e::GitProxy::new("checkout", diffo_e2e::GitGatePhase::After)?;
    let path = gate.path()?;
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("PATH", path.as_os_str())],
    )?;

    screen
        .wait_for_text(&format!(" {original} ·"))?
        .press(Key::Char('1'))?
        .type_text("checkout")?
        .press(Key::Enter)?
        .wait_for_text("Checkout to")?
        .type_text("topic")?
        .wait_for(&Selector::selected_row("topic"))?
        .press(Key::Enter)?
        .wait_for_text("Checking out topic")?;
    gate.wait_until_blocked()?;

    assert_eq!(
        git_output(&repository.worktree, &["branch", "--show-current"])?,
        "topic"
    );
    let blocked_frame = screen.contents();
    assert!(!blocked_frame.contains(" topic ·"));
    assert!(!blocked_frame.contains("Checked out topic"));

    gate.release()?;
    screen
        .wait_for_text(" topic ·")?
        .wait_for_text("Checked out topic")?;
    Ok(())
}

#[test]
fn closing_and_reopening_a_blocked_discovery_ignores_the_first_load() -> Result<()> {
    let repository = TestRepository::new()?;
    git(&repository.worktree, &["branch", "topic"])?;
    let mut gate = diffo_e2e::GitProxy::new("for-each-ref", diffo_e2e::GitGatePhase::After)?;
    let path = gate.path()?;
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("PATH", path.as_os_str())],
    )?;

    screen
        .press(Key::Char('1'))?
        .type_text("checkout")?
        .press(Key::Enter)?
        .wait_for_text("Loading branches...")?;
    gate.wait_until_blocked()?;
    screen
        .press(Key::Escape)?
        .wait_for_text_gone("Checkout to")?
        .press(Key::Char('1'))?
        .type_text("checkout")?
        .press(Key::Enter)?
        .wait_for_text("Loading branches...")?;

    gate.release()?;
    screen.wait_for_text("topic")?;
    assert!(!screen.contents().contains("Could not load branches"));
    Ok(())
}

#[test]
fn watcher_refresh_between_down_and_enter_preserves_the_checkout_target() -> Result<()> {
    let repository = TestRepository::new()?;
    git(&repository.worktree, &["branch", "topic"])?;
    git(&repository.worktree, &["branch", "zzz"])?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .type_text("checkout")?
        .press(Key::Enter)?
        .wait_for_text("Checkout to")?
        .wait_for(&Selector::selected_row("topic"))?
        .press(Key::Down)?
        .wait_for(&Selector::selected_row("zzz"))?;

    fs::write(repository.worktree.join("tracked.txt"), "watcher refresh\n")?;
    screen.wait_for_text("watcher refresh")?;
    screen.press(Key::Enter)?.wait_for_text(" zzz ·")?;
    Ok(())
}
