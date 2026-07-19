use super::support::*;

#[test]
fn commit_modal_commits_then_global_sync_publishes() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(
        repository.worktree.join("tracked.txt"),
        "committed change\n",
    )?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    let mut screen = repository.screen()?;

    screen
        .click(&Selector::text("Update 1 file"))?
        .type_text("Commit from composer")?
        .click(&Selector::dialog_action("Commit message", "Commit"))?;
    wait_for("composer commit", || {
        Ok(
            git_output(&repository.worktree, &["log", "-1", "--format=%s"])?
                == "Commit from composer",
        )
    })?;
    let commit = git_output(&repository.worktree, &["rev-parse", "--short=7", "HEAD"])?;
    screen.wait_for_text(&format!("Committed {commit}"))?;

    screen
        .wait_for_text("[ Sync (9 / F9) ]")?
        .click(&Selector::text("[ Sync (9 / F9) ]"))?;
    wait_for("global sync", || {
        let local = git_output(&repository.worktree, &["rev-parse", "HEAD"])?;
        let remote = git_output(&repository.worktree, &["ls-remote", "origin", "HEAD"])?;
        Ok(remote.starts_with(&local))
    })?;
    screen.wait_for_text("Pushed master.")?;
    screen.wait_for_text_gone("Pushing")?;
    Ok(())
}

#[test]
fn generated_commit_message_commits_staged_changes() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "staged change\n")?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    let mut screen = repository.screen()?;

    screen.wait_for_text("Update 1 file")?.press(Key::Enter)?;
    wait_for("generated commit message", || {
        Ok(git_output(&repository.worktree, &["log", "-1", "--format=%s"])? == "Update 1 file")
    })?;

    Ok(())
}

#[test]
fn commit_input_keeps_focus_across_live_repository_refresh() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "staged change\n")?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    let mut screen = repository.screen()?;

    screen
        .wait_for_text("Update 1 file")?
        .press(Key::Char('e'))?;
    fs::write(repository.worktree.join("new.txt"), "watcher refresh\n")?;
    screen
        .wait_for_text("new.txt")?
        .type_text("Focus survives refresh")?
        .click(&Selector::dialog_action("Commit message", "Commit"))?;

    wait_for("focused composer commit after refresh", || {
        Ok(
            git_output(&repository.worktree, &["log", "-1", "--format=%s"])?
                == "Focus survives refresh",
        )
    })
}

#[test]
fn commit_modal_closes_on_outside_click_and_restores_its_draft() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "staged change\n")?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    let mut screen = repository.screen()?;

    screen
        .click(&Selector::text("Update 1 file"))?
        .wait_for_text("Cancel (Esc)")?
        .type_text("Draft stas")?
        .press(Key::Left)?
        .type_text("y")?
        .click(&Selector::text("Staged"))?
        .wait_for_text_gone("Cancel (Esc)")?
        .click(&Selector::text("Draft stays"))?
        .wait_for_text("Cancel (Esc)")?
        .press(Key::Enter)?;

    wait_for("restored modal draft commit", || {
        Ok(git_output(&repository.worktree, &["log", "-1", "--format=%s"])? == "Draft stays")
    })
}

#[test]
fn disabled_commit_button_does_not_commit_without_staged_changes() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "unstaged change\n")?;
    let before = git_output(&repository.worktree, &["rev-parse", "HEAD"])?;
    let mut screen = repository.screen()?;

    screen
        .click(&Selector::text("Type a message"))?
        .type_text("Must stay uncommitted")?
        .click(&Selector::dialog_action("Commit message", "Commit"))?;
    thread::sleep(Duration::from_millis(150));

    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "HEAD"])?,
        before
    );
    Ok(())
}

#[test]
fn divergent_global_sync_rebases_and_pushes() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("local-one.txt"), "local one\n")?;
    git(&repository.worktree, &["add", "local-one.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Local first"])?;
    fs::write(repository.worktree.join("local-two.txt"), "local two\n")?;
    git(&repository.worktree, &["add", "local-two.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Local second"])?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    git(&repository.worktree, &["fetch", "origin"])?;
    git(&repository.worktree, &["config", "pull.rebase", "false"])?;
    git(&repository.worktree, &["config", "pull.ff", "false"])?;
    let before = git_output(&repository.worktree, &["rev-parse", "HEAD"])?;
    let mut gate = diffo_e2e::GitProxy::new("rebase", diffo_e2e::GitGatePhase::Before)?;
    let path = gate.path()?;
    let mut screen = DiffoScreen::launch_with_env(
        diffo_binary()?,
        &repository.worktree,
        &[("PATH", path.as_os_str())],
    )?;

    screen
        .wait_for_text("[ Sync (9 / F9) ]")?
        .click(&Selector::text("[ Sync (9 / F9) ]"))?;
    gate.wait_until_blocked()?;
    screen
        .wait_for_text("origin/master has 1 upstream-only")?
        .wait_for_text("master has 2 local-only commits.")?
        .wait_for_text("Plan:")?
        .wait_for_text("rebase 2 commits onto")?
        .wait_for_text("push.")?
        .wait_for_text("Rebasing 2 commits")?;
    gate.release()?;
    screen.wait_for_text("Rebased 2 commits and pushed master.")?;

    let after = git_output(&repository.worktree, &["rev-parse", "HEAD"])?;
    assert_ne!(after, before);
    assert!(repository.worktree.join("remote.txt").exists());
    assert_eq!(
        git_output(
            &repository.worktree,
            &["log", "-3", "--reverse", "--format=%s", "HEAD"]
        )?,
        "Remote commit\nLocal first\nLocal second"
    );
    assert!(
        git_output(
            &repository.worktree,
            &["rev-list", "--min-parents=2", "HEAD"]
        )?
        .is_empty()
    );
    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "HEAD"])?,
        after
    );
    assert_eq!(
        git_output(
            &repository.root.path().join("remote.git"),
            &["rev-parse", "HEAD"]
        )?,
        after
    );
    Ok(())
}
