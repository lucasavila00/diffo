use super::support::*;

const QUIET: Duration = Duration::from_millis(150);
const WATCHER_QUIET: Duration = Duration::from_millis(750);

fn changed_screen() -> Result<(TestRepository, DiffoScreen)> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    let screen = repository.screen()?;
    Ok((repository, screen))
}

fn expect_key_quiet(screen: &mut DiffoScreen, key: Key) -> Result<()> {
    screen
        .press(key)?
        .expect_quiet(QUIET)
        .with_context(|| format!("key {key:?} caused terminal output"))?;
    Ok(())
}

#[test]
fn steady_state_is_silent_across_multiple_idle_poll_windows() -> Result<()> {
    let (_repository, mut screen) = changed_screen()?;

    screen
        .wait_for_quiet(QUIET)?
        .expect_quiet(Duration::from_millis(450))?;
    Ok(())
}

#[test]
fn unbound_and_uppercase_diff_keys_are_silent() -> Result<()> {
    let (_repository, mut screen) = changed_screen()?;
    screen.wait_for_quiet(QUIET)?;

    for key in ['O', 'D', 'R', 'F', 'Z'] {
        expect_key_quiet(&mut screen, Key::Char(key))?;
    }
    Ok(())
}

#[test]
fn diff_scroll_boundaries_are_silent_when_everything_fits() -> Result<()> {
    let (_repository, mut screen) = changed_screen()?;
    screen.wait_for_quiet(QUIET)?;

    for key in [
        Key::Up,
        Key::PageUp,
        Key::Left,
        Key::Down,
        Key::PageDown,
        Key::Right,
    ] {
        expect_key_quiet(&mut screen, key)?;
    }
    Ok(())
}

#[test]
fn previous_change_at_the_first_change_is_silent() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(
        repository.worktree.join("tracked.txt"),
        navigation_file(false)?,
    )?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Navigation baseline"],
    )?;
    fs::write(
        repository.worktree.join("tracked.txt"),
        navigation_file(true)?,
    )?;
    let mut screen = repository.screen()?;
    screen
        .wait_for_text("FIRST_CHANGE")?
        .wait_for_quiet(QUIET)?;

    expect_key_quiet(&mut screen, Key::Char('p'))
}

#[test]
fn next_change_at_the_last_change_is_silent() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(
        repository.worktree.join("tracked.txt"),
        navigation_file(false)?,
    )?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    git(
        &repository.worktree,
        &["commit", "-m", "Navigation baseline"],
    )?;
    fs::write(
        repository.worktree.join("tracked.txt"),
        navigation_file(true)?,
    )?;
    let mut screen = repository.screen()?;
    screen
        .press(Key::Char('n'))?
        .wait_for_text("MIDDLE_CHANGE")?
        .press(Key::Char('n'))?
        .wait_for_text("LAST_CHANGE")?
        .wait_for_quiet(QUIET)?;

    expect_key_quiet(&mut screen, Key::Char('n'))
}

#[test]
fn clicking_the_selected_file_again_is_silent() -> Result<()> {
    let (_repository, mut screen) = changed_screen()?;
    screen
        .wait_for(&Selector::selected_row("tracked.txt"))?
        .wait_for_quiet(QUIET)?
        .click(&Selector::selected_row("tracked.txt"))?
        .expect_quiet(QUIET)?;
    Ok(())
}

#[test]
fn clicking_inert_activity_rail_space_is_silent() -> Result<()> {
    let (_repository, mut screen) = changed_screen()?;
    screen
        .wait_for_quiet(QUIET)?
        .click_at(2, 20)?
        .expect_quiet(QUIET)?;
    Ok(())
}

#[test]
fn clicking_the_already_selected_activity_is_silent() -> Result<()> {
    let (_repository, mut screen) = changed_screen()?;
    screen
        .wait_for_quiet(QUIET)?
        .click(&Selector::text(""))?
        .expect_quiet(QUIET)?;
    Ok(())
}

#[test]
fn explorer_viewer_boundaries_are_silent_when_everything_fits() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut screen = repository.screen()?;
    screen
        .press(Key::Tab)?
        .wait_for_text("Explorer")?
        .wait_for_text("base")?
        .wait_for_quiet(QUIET)?;

    for key in [
        Key::Up,
        Key::PageUp,
        Key::Left,
        Key::Down,
        Key::PageDown,
        Key::Right,
    ] {
        expect_key_quiet(&mut screen, key)?;
    }
    Ok(())
}

#[test]
fn rewriting_identical_contents_does_not_redraw() -> Result<()> {
    let (repository, mut screen) = changed_screen()?;
    screen.wait_for_quiet(QUIET)?;

    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    screen.expect_quiet(WATCHER_QUIET)?;
    Ok(())
}

#[test]
fn inactive_explorer_refresh_does_not_redraw_diff() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join(".gitignore"), "*.ignored\n")?;
    git(&repository.worktree, &["add", ".gitignore"])?;
    git(&repository.worktree, &["commit", "-m", "Ignore test files"])?;
    let mut screen = repository.screen()?;
    screen.wait_for_quiet(QUIET)?;

    fs::write(repository.worktree.join("new.ignored"), "ignored\n")?;
    screen.expect_quiet(WATCHER_QUIET)?;
    Ok(())
}

#[test]
fn diff_file_navigation_boundaries_are_silent_with_one_file() -> Result<()> {
    let (_repository, mut screen) = changed_screen()?;
    screen
        .wait_for(&Selector::selected_row("tracked.txt"))?
        .wait_for_quiet(QUIET)?;

    for key in [
        Key::Home,
        Key::End,
        Key::Char('g'),
        Key::Char('j'),
        Key::Char('k'),
        Key::Char('l'),
    ] {
        expect_key_quiet(&mut screen, key)?;
    }
    Ok(())
}

#[test]
fn explorer_file_navigation_boundaries_are_silent_with_one_file() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut screen = repository.screen()?;
    screen
        .press(Key::Tab)?
        .wait_for_text("Explorer")?
        .wait_for_text("base")?
        .wait_for_quiet(QUIET)?;

    for key in [
        Key::Home,
        Key::End,
        Key::Char('g'),
        Key::Char('j'),
        Key::Char('k'),
        Key::Char('l'),
    ] {
        expect_key_quiet(&mut screen, key)?;
    }
    Ok(())
}

#[test]
fn mouse_wheel_at_the_top_boundary_is_silent() -> Result<()> {
    let (_repository, mut screen) = changed_screen()?;
    screen
        .wait_for_quiet(QUIET)?
        .scroll(ScrollDirection::Up)?
        .expect_quiet(QUIET)?;
    Ok(())
}

#[test]
fn full_screen_diff_boundaries_are_silent_when_everything_fits() -> Result<()> {
    let (_repository, mut screen) = changed_screen()?;
    screen
        .press(Key::Char('f'))?
        .wait_for_text_gone("Changes")?
        .wait_for_quiet(QUIET)?;

    for key in [
        Key::Up,
        Key::PageUp,
        Key::Left,
        Key::Down,
        Key::PageDown,
        Key::Right,
    ] {
        expect_key_quiet(&mut screen, key)?;
    }
    Ok(())
}

#[test]
fn full_screen_explorer_boundaries_are_silent_when_everything_fits() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut screen = repository.screen()?;
    screen
        .press(Key::Tab)?
        .wait_for_text("Explorer")?
        .wait_for_text("base")?
        .press(Key::Char('f'))?
        .wait_for_text_gone("Explorer")?
        .wait_for_quiet(QUIET)?;

    for key in [
        Key::Up,
        Key::PageUp,
        Key::Left,
        Key::Down,
        Key::PageDown,
        Key::Right,
    ] {
        expect_key_quiet(&mut screen, key)?;
    }
    Ok(())
}

#[test]
fn clean_explorer_stays_silent_after_identical_worker_results() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut screen = repository.screen()?;
    screen
        .press(Key::Tab)?
        .wait_for_text("Explorer")?
        .wait_for_text("base")?
        .wait_for_quiet(QUIET)?
        .expect_quiet(Duration::from_millis(450))?;
    Ok(())
}

#[test]
fn empty_diff_navigation_and_scrolling_are_silent() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut screen = repository.screen()?;
    screen.wait_for_text("No files.")?.wait_for_quiet(QUIET)?;

    for key in [
        Key::Home,
        Key::End,
        Key::Up,
        Key::Down,
        Key::PageUp,
        Key::PageDown,
        Key::Left,
        Key::Right,
        Key::Char('g'),
        Key::Char('j'),
        Key::Char('k'),
        Key::Char('l'),
        Key::Char('n'),
        Key::Char('p'),
    ] {
        expect_key_quiet(&mut screen, key)?;
    }
    Ok(())
}
