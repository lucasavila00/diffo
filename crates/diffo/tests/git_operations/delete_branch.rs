use super::support::*;

#[test]
fn merged_branch_deletes_from_the_command_palette() -> Result<()> {
    let repository = TestRepository::new()?;
    git(&repository.worktree, &["branch", "merged"])?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .type_text("delete branch")?
        .press(Key::Enter)?
        .wait_for_text("Delete branch")?
        .wait_for_text_gone("Loading branches...")?
        .type_text("merged")?
        .wait_for(&Selector::selected_row("merged"))?
        .press(Key::Enter)?
        .wait_for_text("Deleted branch merged")?;

    git_must_fail(
        &repository.worktree,
        &["show-ref", "--verify", "refs/heads/merged"],
    )?;
    Ok(())
}

#[test]
fn unmerged_branch_requires_explicit_force_confirmation() -> Result<()> {
    let repository = TestRepository::new()?;
    let original = git_output(&repository.worktree, &["branch", "--show-current"])?;
    git(&repository.worktree, &["switch", "-c", "topic"])?;
    fs::write(repository.worktree.join("topic.txt"), "topic\n")?;
    git(&repository.worktree, &["add", "topic.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Topic"])?;
    git(&repository.worktree, &["switch", &original])?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .type_text("delete branch")?
        .press(Key::Enter)?
        .wait_for_text_gone("Loading branches...")?
        .type_text("topic")?
        .wait_for(&Selector::selected_row("topic"))?
        .press(Key::Enter)?
        .wait_for_text("not fully merged")?
        .press(Key::Enter)?
        .wait_for_text_gone("not fully merged")?;
    git_output(
        &repository.worktree,
        &["show-ref", "--verify", "refs/heads/topic"],
    )?;
    assert!(!screen.contents().contains("Delete branch failed"));

    screen
        .press(Key::Char('1'))?
        .type_text("delete branch")?
        .press(Key::Enter)?
        .wait_for_text_gone("Loading branches...")?
        .type_text("topic")?
        .wait_for(&Selector::selected_row("topic"))?
        .press(Key::Enter)?
        .wait_for_text("not fully merged")?
        .press(Key::Right)?
        .press(Key::Enter)?
        .wait_for_text("Deleted branch topic")?;
    git_must_fail(
        &repository.worktree,
        &["show-ref", "--verify", "refs/heads/topic"],
    )?;
    Ok(())
}

#[test]
fn cancelling_before_mutation_preserves_the_selected_branch() -> Result<()> {
    let repository = TestRepository::new()?;
    git(&repository.worktree, &["branch", "merged"])?;
    let gate = diffo_e2e::GitProxy::new("branch", diffo_e2e::GitGatePhase::Before)?;
    let path = gate.path()?;
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("PATH", path.as_os_str())],
    )?;

    screen
        .press(Key::Char('1'))?
        .type_text("delete branch")?
        .press(Key::Enter)?
        .wait_for_text_gone("Loading branches...")?
        .type_text("merged")?
        .wait_for(&Selector::selected_row("merged"))?
        .press(Key::Enter)?
        .wait_for_text("Deleting branch merged")?;
    gate.wait_until_blocked()?;
    screen
        .click(&Selector::toast_action("Deleting branch merged", ""))?
        .wait_for_text_gone("Deleting branch merged")?;

    git_output(
        &repository.worktree,
        &["show-ref", "--verify", "refs/heads/merged"],
    )?;
    assert!(!screen.contents().contains("Deleted branch merged"));
    Ok(())
}

#[test]
fn moved_branch_is_rejected_before_deletion() -> Result<()> {
    let repository = TestRepository::new()?;
    git(&repository.worktree, &["branch", "topic"])?;
    fs::write(repository.worktree.join("later.txt"), "later\n")?;
    git(&repository.worktree, &["add", "later.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Advance main"])?;
    let mut gate = diffo_e2e::GitProxy::new("show-ref", diffo_e2e::GitGatePhase::Before)?;
    let path = gate.path()?;
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("PATH", path.as_os_str())],
    )?;

    screen
        .press(Key::Char('1'))?
        .type_text("delete branch")?
        .press(Key::Enter)?
        .wait_for_text_gone("Loading branches...")?
        .type_text("topic")?
        .wait_for(&Selector::selected_row("topic"))?
        .press(Key::Enter)?;
    gate.wait_until_blocked()?;
    git(&repository.worktree, &["branch", "-f", "topic", "HEAD"])?;
    gate.release()?;
    screen
        .wait_for_text("Delete branch failed: selected branch")?
        .wait_for_text("changed; reopen the branch picker")?;

    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "refs/heads/topic"])?,
        git_output(&repository.worktree, &["rev-parse", "HEAD"])?
    );
    Ok(())
}
