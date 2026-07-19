use super::support::*;

#[test]
fn opens_the_first_unstaged_file_when_staged_files_exist() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "staged\n")?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    fs::write(repository.worktree.join("review.txt"), "review me\n")?;
    let mut screen = repository.screen()?;

    screen
        .wait_for(&Selector::selected_row("review.txt"))?
        .wait_for_text("review me")?;
    Ok(())
}

#[test]
fn space_stages_selected_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    let mut screen = repository.screen()?;

    screen.press(Key::Char(' '))?;

    wait_for("tracked.txt to be staged", || {
        Ok(cached_paths(&repository.worktree)?.contains("tracked.txt"))
    })
}

#[test]
fn slow_stage_shows_progress_then_commits_only_the_list_change() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    let mut gate = diffo_e2e::GitProxy::new("add", diffo_e2e::GitGatePhase::Before)?;
    let path = gate.path()?;
    let mut screen = DiffoScreen::launch_with_env(
        env!("CARGO_BIN_EXE_diffo"),
        &repository.worktree,
        &[("PATH", path.as_os_str())],
    )?;

    screen
        .wait_for(&Selector::selected_row("tracked.txt"))?
        .press(Key::Char(' '))?;
    gate.wait_until_blocked()?;
    screen.wait_for_text("Staging")?;
    gate.release()?;
    screen
        .wait_for(&Selector::file_action("Staged", "tracked.txt", "[-]"))?
        .wait_for_text_gone("Staging")?;
    assert!(!screen.contents().contains("Stage complete"));
    Ok(())
}

#[test]
fn space_stages_and_selects_the_next_unstaged_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    fs::write(repository.worktree.join("next.txt"), "next\n")?;
    let mut screen = repository.screen()?;

    screen
        .wait_for(&Selector::selected_row("tracked.txt"))?
        .press(Key::Char(' '))?;
    wait_for("tracked.txt to be staged", || {
        Ok(cached_paths(&repository.worktree)?.contains("tracked.txt"))
    })?;
    screen.wait_for(&Selector::selected_row("next.txt"))?;
    Ok(())
}

#[test]
fn space_unstages_selected_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    let mut screen = repository.screen()?;

    screen.press(Key::Char(' '))?;

    wait_for("tracked.txt to be unstaged", || {
        Ok(!cached_paths(&repository.worktree)?.contains("tracked.txt"))
    })
}

#[test]
fn a_stages_all_files() -> Result<()> {
    let repository = changed_repository()?;
    let mut screen = repository.screen()?;

    screen.press(Key::Char('a'))?;

    wait_for("all files to be staged", || {
        all_changes_are_staged(&repository.worktree)
    })
}

#[test]
fn a_unstages_all_files() -> Result<()> {
    let repository = changed_repository()?;
    git(&repository.worktree, &["add", "."])?;
    let mut screen = repository.screen()?;

    screen.press(Key::Char('a'))?;

    wait_for("all files to be unstaged", || {
        Ok(cached_paths(&repository.worktree)?.is_empty())
    })
}

#[test]
fn changes_header_stages_all_files() -> Result<()> {
    let repository = changed_repository()?;
    let mut screen = repository.screen()?;

    screen.click(&Selector::panel_action("Changes", "+"))?;

    wait_for("header action to stage all files", || {
        all_changes_are_staged(&repository.worktree)
    })
}

#[test]
fn staged_header_unstages_all_files() -> Result<()> {
    let repository = changed_repository()?;
    git(&repository.worktree, &["add", "."])?;
    let mut screen = repository.screen()?;

    screen.click(&Selector::panel_action("Staged", "-"))?;

    wait_for("header action to unstage all files", || {
        Ok(cached_paths(&repository.worktree)?.is_empty())
    })
}

#[test]
fn plus_button_stages_clicked_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    let mut screen = repository.screen()?;

    screen.click(&Selector::file_action("Changes", "tracked.txt", "[+]"))?;

    wait_for("clicked file to be staged", || {
        Ok(cached_paths(&repository.worktree)?.contains("tracked.txt"))
    })
}

#[test]
fn minus_button_unstages_clicked_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    let mut screen = repository.screen()?;

    screen.click(&Selector::file_action("Staged", "tracked.txt", "[-]"))?;

    wait_for("clicked file to be unstaged", || {
        Ok(!cached_paths(&repository.worktree)?.contains("tracked.txt"))
    })
}
