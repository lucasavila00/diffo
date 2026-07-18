use super::support::*;

#[test]
fn mock_remote_error_shows_the_executed_action() -> Result<()> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../diffo-core/fixtures/repository-state.ron");
    let mut screen = DiffoScreen::launch_with_env(
        env!("CARGO_BIN_EXE_diffo"),
        Path::new(env!("CARGO_MANIFEST_DIR")),
        &[
            ("DIFFO_MOCK_FILE", fixture.as_os_str()),
            ("DIFFO_MOCK_MUTABLE", OsStr::new("1")),
        ],
    )?;

    screen
        .press(Key::Char('1'))?
        .type_text("fetch")?
        .press(Key::Enter)?
        .wait_for_text("Fetch failed:")?
        .wait_for_text("Fetch")?;
    Ok(())
}

#[test]
fn palette_search_runs_fetch() -> Result<()> {
    let repository = TestRepository::new()?;
    let remote_commit = repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let mut screen = repository.screen_with_network_delay()?;

    screen
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("fetch")?
        .press(Key::Enter)?
        .wait_for_text("Fetching")?;

    wait_for("origin tracking branch to be fetched", || {
        Ok(git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])? == remote_commit)
    })?;
    screen.wait_for_text("Fetched 1 ref")?;
    screen.wait_for_text_gone("Fetching")?;
    assert!(!repository.worktree.join("remote.txt").exists());
    Ok(())
}

#[test]
fn palette_search_runs_pull() -> Result<()> {
    let repository = TestRepository::new()?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let mut screen = repository.screen_with_network_delay()?;

    screen
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("pull")?
        .press(Key::Enter)?
        .wait_for_text("Pulling")?;

    wait_for("remote file to be pulled", || {
        Ok(repository.worktree.join("remote.txt").exists())
    })?;
    screen.wait_for_text("Pulled 1 commit")?;
    screen.wait_for_text_gone("Pulling")?;
    Ok(())
}

#[test]
fn primary_pull_button_shows_loading_and_pulls() -> Result<()> {
    let repository = TestRepository::new()?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    git(&repository.worktree, &["fetch", "origin"])?;
    let mut screen = repository.screen_with_network_delay()?;

    screen
        .wait_for_text("[ Pull ]")?
        .click(&Selector::text("[ Pull ]"))?
        .wait_for_text("Pulling")?;
    wait_for("primary pull to update the worktree", || {
        Ok(repository.worktree.join("remote.txt").exists())
    })?;
    screen.wait_for_text_gone("Pulling")?;
    Ok(())
}

#[test]
fn rejected_push_shows_a_persistent_failure_toast() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("local.txt"), "local\n")?;
    git(&repository.worktree, &["add", "local.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Local commit"])?;
    let mut screen = repository.screen_with_network_delay()?;
    screen.wait_for_text("[ Push ]")?;

    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    screen
        .click(&Selector::text("[ Push ]"))?
        .wait_for_text("Pushing")?
        .wait_for_text("Push rejected: remote changed")?;
    thread::sleep(Duration::from_millis(300));
    assert!(screen.contents().contains("Push rejected"));
    Ok(())
}

#[test]
fn success_toast_is_automatically_dismissed() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    let mut screen = repository.screen()?;

    screen
        .click(&Selector::text("[ Commit ]"))?
        .wait_for_text("Committed ")?
        .wait_for_text_gone("Committed ")?;
    Ok(())
}
