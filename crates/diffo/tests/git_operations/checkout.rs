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
        env!("CARGO_BIN_EXE_diffo"),
        &repository.worktree,
        &[("PATH", path.as_os_str())],
    )?;

    screen
        .wait_for_text(&format!("branch {original}"))?
        .press(Key::Char('1'))?
        .type_text("checkout")?
        .press(Key::Enter)?
        .wait_for_text("Checkout to")?
        .type_text("topic")?
        .press(Key::Enter)?
        .wait_for_text("Checking out topic")?;
    gate.wait_until_blocked()?;

    assert_eq!(
        git_output(&repository.worktree, &["branch", "--show-current"])?,
        "topic"
    );
    let blocked_frame = screen.contents();
    assert!(!blocked_frame.contains("branch topic"));
    assert!(!blocked_frame.contains("Checked out topic"));

    gate.release()?;
    screen
        .wait_for_text("branch topic")?
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
        env!("CARGO_BIN_EXE_diffo"),
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
