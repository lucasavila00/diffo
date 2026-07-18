use super::support::*;

#[test]
fn commit_composer_commits_then_pushes() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(
        repository.worktree.join("tracked.txt"),
        "committed change\n",
    )?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    let mut screen = repository.screen_with_network_delay()?;

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
        .wait_for_text("[ Push ]")?
        .click(&Selector::text("[ Push ]"))?
        .wait_for_text("Pushing")?;
    wait_for("composer push", || {
        let local = git_output(&repository.worktree, &["rev-parse", "HEAD"])?;
        let remote = git_output(&repository.worktree, &["ls-remote", "origin", "HEAD"])?;
        Ok(remote.starts_with(&local))
    })?;
    screen.wait_for_text(&format!("Pushed {commit} to origin/master"))?;
    screen.wait_for_text_gone("Pushing")?;
    Ok(())
}

#[test]
fn generated_commit_message_commits_staged_changes() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "staged change\n")?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    let mut screen = repository.screen()?;

    screen
        .wait_for_text("Update 1 file")?
        .wait_for_text("[ Commit ]")?
        .click(&Selector::text("[ Commit ]"))?;
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
        .click(&Selector::text("Update 1 file"))?;
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
        .wait_for_text("Esc: cancel")?
        .type_text("Draft stas")?
        .press(Key::Left)?
        .type_text("y")?
        .click(&Selector::text("Staged"))?
        .wait_for_text_gone("Esc: cancel")?
        .click(&Selector::text("Draft stays"))?
        .wait_for_text("Esc: cancel")?
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
fn divergent_primary_button_is_blocked() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("local.txt"), "local\n")?;
    git(&repository.worktree, &["add", "local.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Local commit"])?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    git(&repository.worktree, &["fetch", "origin"])?;
    let before = git_output(&repository.worktree, &["rev-parse", "HEAD"])?;
    let mut screen = repository.screen()?;

    screen
        .wait_for_text("[ Push + Pull ]")?
        .click(&Selector::text("[ Push + Pull ]"))?
        .wait_for_text("Push blocked: pull and merge required")?;
    thread::sleep(Duration::from_millis(150));

    assert_eq!(
        git_output(&repository.worktree, &["rev-parse", "HEAD"])?,
        before
    );
    assert!(!repository.worktree.join("remote.txt").exists());
    Ok(())
}
