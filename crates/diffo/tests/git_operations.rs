use std::{
    ffi::OsStr,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use diffo_e2e::{DiffoScreen, Key, ScrollDirection, Selector};
use serde::Deserialize;

const TIMEOUT: Duration = Duration::from_secs(5);

#[test]
fn mock_renamed_file_renders_unchanged_content() -> Result<()> {
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
        .wait_for_text("src/empty-and-r")?
        .click(&Selector::text("src/content-and"))?
        .wait_for_text("pub struct RenamedFile")?
        .wait_for_text("Content is unchanged by the rename")?;
    Ok(())
}

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
        .wait_for_text("cannot execute Fetch: no remote configured")?;
    Ok(())
}

#[test]
fn real_merge_conflict_renders_as_a_highlighted_worktree_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(
        repository.worktree.join("tracked.txt"),
        "fn value() -> i32 { 1 }\n",
    )?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    git(&repository.worktree, &["commit", "-m", "Local edit"])?;
    repository.commit_remote("tracked.txt", "fn value() -> i32 { 2 }\n", "Remote edit")?;
    git(&repository.worktree, &["fetch", "origin"])?;
    git_must_fail(&repository.worktree, &["merge", "origin/master"])?;

    let mut screen = repository.screen()?;
    screen
        .wait_for_text("U  tracked.txt")?
        .wait_for_text("<<<<<<< HEAD")?
        .wait_for_text("=======")?
        .wait_for_text(">>>>>>> origin/master")?;
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

#[test]
fn palette_search_runs_fetch() -> Result<()> {
    let repository = TestRepository::new()?;
    let remote_commit = repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("fetch")?
        .press(Key::Enter)?;

    wait_for("origin tracking branch to be fetched", || {
        Ok(git_output(&repository.worktree, &["rev-parse", "origin/HEAD"])? == remote_commit)
    })?;
    assert!(!repository.worktree.join("remote.txt").exists());
    Ok(())
}

#[test]
fn palette_search_runs_pull() -> Result<()> {
    let repository = TestRepository::new()?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("pull")?
        .press(Key::Enter)?;

    wait_for("remote file to be pulled", || {
        Ok(repository.worktree.join("remote.txt").exists())
    })
}

#[test]
fn clicking_palette_result_runs_command() -> Result<()> {
    let repository = TestRepository::new()?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("pull")?
        .click(&Selector::text("Git: Pull"))?;

    wait_for("clicked pull command to finish", || {
        Ok(repository.worktree.join("remote.txt").exists())
    })
}

#[test]
fn overlays_open_and_close_with_function_keys() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Function(1))?
        .wait_for_text("Command Palette")?
        .press(Key::Escape)?
        .wait_for_text_gone("Command Palette")?
        .press(Key::Function(2))?
        .wait_for_text("Help")?
        .wait_for_text("Shortcut")?
        .wait_for_text("Action")?
        .wait_for_text("k / l / s")?
        .wait_for_text("Next file")?
        .wait_for_text("Page Up")?
        .wait_for_text("Scroll up one page")?
        .wait_for_text("Stage / unstage selected file")?
        .press(Key::Function(2))?
        .wait_for_text_gone("Help")?
        .press(Key::Char('2'))?
        .wait_for_text("Help")?
        .press(Key::Char('2'))?
        .wait_for_text_gone("Help")?;
    Ok(())
}

#[test]
fn mouse_click_selects_a_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(
        repository.worktree.join("tracked.txt"),
        "tracked selected\n",
    )?;
    fs::write(repository.worktree.join("new.txt"), "new selected\n")?;
    let mut screen = repository.screen()?;

    screen
        .click(&Selector::text("new.txt"))?
        .wait_for(&Selector::selected_row("new.txt"))?
        .wait_for_text("new selected")?;
    Ok(())
}

#[test]
fn view_and_file_pane_toggles_render_immediately() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    let mut screen = repository.screen()?;

    screen
        .press(Key::Char('r'))?
        .wait_for_text("Side by side")?
        .press(Key::Char('e'))?
        .wait_for_text_gone("Changes")?;
    assert!(screen.contents().contains("File Diff"));
    screen.press(Key::Char('e'))?.wait_for_text("Changes")?;
    Ok(())
}

#[test]
fn keyboard_and_mouse_scroll_move_the_visible_diff() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut contents = String::new();
    for line in 0..120 {
        writeln!(contents, "line {line:03}").context("build scrolling fixture")?;
    }
    fs::write(repository.worktree.join("tracked.txt"), contents)?;
    let mut screen = repository.screen()?;
    screen.wait_for_text("line 000")?;

    screen
        .press(Key::Down)?
        .wait_for_text_gone("line 000")?
        .press(Key::Up)?
        .wait_for_text("line 000")?
        .press(Key::PageDown)?
        .wait_for_text_gone("line 000")?
        .press(Key::PageUp)?
        .wait_for_text("line 000")?
        .scroll_many(ScrollDirection::Down, 4)?
        .wait_for_text_gone("line 000")?
        .scroll_many(ScrollDirection::Up, 4)?
        .wait_for_text("line 000")?
        .drag_vertical_scrollbar(0, 50)?
        .wait_for_text_gone("line 000")?
        .drag_vertical_scrollbar(50, 0)?
        .wait_for_text("line 000")?;
    Ok(())
}

#[test]
fn every_file_navigation_alias_moves_selection() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    fs::write(repository.worktree.join("new.txt"), "new\n")?;
    let mut screen = repository.screen()?;
    screen.wait_for(&Selector::selected_row("tracked.txt"))?;

    screen
        .press(Key::End)?
        .wait_for(&Selector::selected_row("new.txt"))?
        .press(Key::Home)?
        .wait_for(&Selector::selected_row("tracked.txt"))?
        .press(Key::Char('G'))?
        .wait_for(&Selector::selected_row("new.txt"))?
        .press(Key::Char('g'))?
        .wait_for(&Selector::selected_row("tracked.txt"))?
        .press(Key::Char('s'))?
        .wait_for(&Selector::selected_row("new.txt"))?
        .press(Key::Char('w'))?
        .wait_for(&Selector::selected_row("tracked.txt"))?
        .press(Key::Char('k'))?
        .wait_for(&Selector::selected_row("new.txt"))?
        .press(Key::Char('j'))?
        .wait_for(&Selector::selected_row("tracked.txt"))?
        .press(Key::Char('l'))?
        .wait_for(&Selector::selected_row("new.txt"))?
        .press(Key::Char('j'))?
        .wait_for(&Selector::selected_row("tracked.txt"))?;
    Ok(())
}

#[test]
fn q_and_control_c_exit_cleanly() -> Result<()> {
    let repository = TestRepository::new()?;
    repository
        .screen()?
        .press(Key::Char('q'))?
        .wait_for_exit()?;
    repository
        .screen()?
        .press(Key::Ctrl('c'))?
        .wait_for_exit()?;
    Ok(())
}

#[test]
fn wheel_burst_is_one_bounded_frame_transition() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut contents = String::new();
    for line in 0..200 {
        writeln!(contents, "line {line:03}").context("build trace fixture")?;
    }
    fs::write(repository.worktree.join("tracked.txt"), contents)?;
    let trace_path = repository.root.path().join("frames.ronl");
    let mut screen = DiffoScreen::launch_with_env(
        env!("CARGO_BIN_EXE_diffo"),
        &repository.worktree,
        &[("DIFFO_TRACE_FRAMES", trace_path.as_os_str())],
    )?;
    screen
        .wait_for_text("line 000")?
        .scroll_many(ScrollDirection::Down, 10)?
        .wait_for_text_gone("line 000")?
        .scroll_many(ScrollDirection::Up, 10)?
        .wait_for_text("line 000")?
        .press_many(Key::Down, 10)?
        .wait_for_text_gone("line 000")?
        .press(Key::Char('q'))?
        .wait_for_exit()?;
    drop(screen);

    let trace = fs::read_to_string(&trace_path).context("read frame trace")?;
    let records = trace
        .lines()
        .map(ron::from_str::<ScrollFrame>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let frame = records
        .iter()
        .find(|record| {
            record
                .input_events
                .iter()
                .filter(|event| event.contains("ScrollDown"))
                .count()
                == 10
        })
        .with_context(|| format!("trace has no coalesced wheel frame:\n{trace}"))?;
    assert_eq!(
        frame.scroll_after.0.saturating_sub(frame.scroll_before.0),
        10
    );
    let input_to_draw = frame.draw_end_us.saturating_sub(
        frame
            .event_read_us
            .context("wheel frame has no event time")?,
    );
    assert!(
        input_to_draw < 250_000,
        "input-to-draw took {input_to_draw}µs"
    );
    let key_frame = records
        .iter()
        .find(|record| {
            record
                .input_events
                .iter()
                .filter(|event| event.contains("code: Down"))
                .count()
                == 10
        })
        .with_context(|| format!("trace has no coalesced key-repeat frame:\n{trace}"))?;
    assert_eq!(
        key_frame
            .scroll_after
            .0
            .saturating_sub(key_frame.scroll_before.0),
        40
    );
    Ok(())
}

#[test]
fn live_content_change_keeps_the_visible_line_anchored() -> Result<()> {
    let repository = TestRepository::new()?;
    let contents = numbered_lines(120, false)?;
    fs::write(repository.worktree.join("tracked.txt"), contents)?;
    let mut screen = repository.screen()?;
    screen
        .wait_for_text("line 000")?
        .press_many(Key::Down, 10)?
        .wait_for_text("line 038")?;
    let anchor = Selector::text("line 038");
    let before = screen
        .position(&anchor)?
        .context("anchor line is not visible before refresh")?;

    let mut changed = String::new();
    for index in 0..5 {
        writeln!(changed, "inserted {index}").context("build inserted lines")?;
    }
    changed.push_str(&numbered_lines(120, true)?);
    fs::write(repository.worktree.join("tracked.txt"), changed)?;
    screen.wait_for_text("changed neighbor")?;
    let after = screen
        .position(&anchor)?
        .context("anchor line is not visible after refresh")?;

    assert_eq!(
        after.1, before.1,
        "content refresh moved the visible anchor"
    );
    Ok(())
}

fn numbered_lines(count: usize, change_neighbor: bool) -> Result<String> {
    let mut contents = String::new();
    for line in 0..count {
        if change_neighbor && line == 39 {
            writeln!(contents, "changed neighbor").context("build changed line")?;
        } else {
            writeln!(contents, "line {line:03}").context("build numbered line")?;
        }
    }
    Ok(contents)
}

#[derive(Deserialize)]
struct ScrollFrame {
    input_events: Vec<String>,
    scroll_before: (usize, usize),
    scroll_after: (usize, usize),
    event_read_us: Option<u64>,
    draw_end_us: u64,
}

fn changed_repository() -> Result<TestRepository> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    fs::write(repository.worktree.join("new.txt"), "new\n")?;
    Ok(repository)
}

fn all_changes_are_staged(repository: &Path) -> Result<bool> {
    let paths = cached_paths(repository)?;
    Ok(paths.contains("tracked.txt") && paths.contains("new.txt"))
}

fn cached_paths(repository: &Path) -> Result<String> {
    git_output(repository, &["diff", "--cached", "--name-only"])
}

fn wait_for(description: &str, mut condition: impl FnMut() -> Result<bool>) -> Result<()> {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        if condition()? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
    bail!("timed out waiting for {description}")
}

struct TestRepository {
    root: tempfile::TempDir,
    seed: PathBuf,
    worktree: PathBuf,
}

impl TestRepository {
    fn new() -> Result<Self> {
        let root = tempfile::tempdir().context("create repository root")?;
        git(root.path(), &["init", "--bare", "remote.git"])?;
        git(root.path(), &["clone", "remote.git", "seed"])?;
        let seed = root.path().join("seed");
        configure(&seed)?;
        fs::write(seed.join("tracked.txt"), "base\n")?;
        git(&seed, &["add", "."])?;
        git(&seed, &["commit", "-m", "Base commit"])?;
        git(&seed, &["push", "-u", "origin", "HEAD"])?;
        git(root.path(), &["clone", "remote.git", "work"])?;
        let worktree = root.path().join("work");
        configure(&worktree)?;
        Ok(Self {
            root,
            seed,
            worktree,
        })
    }

    fn screen(&self) -> Result<DiffoScreen> {
        DiffoScreen::launch(env!("CARGO_BIN_EXE_diffo"), &self.worktree)
    }

    fn commit_remote(&self, path: &str, contents: &str, message: &str) -> Result<String> {
        fs::write(self.seed.join(path), contents)?;
        git(&self.seed, &["add", path])?;
        git(&self.seed, &["commit", "-m", message])?;
        git(&self.seed, &["push", "origin", "HEAD"])?;
        git_output(&self.seed, &["rev-parse", "HEAD"])
    }
}

fn configure(repository: &Path) -> Result<()> {
    git(repository, &["config", "user.name", "Diffo Test"])?;
    git(
        repository,
        &["config", "user.email", "diffo@example.invalid"],
    )
}

fn git(repository: &Path, args: &[&str]) -> Result<()> {
    git_command(repository, args).map(|_| ())
}

fn git_output(repository: &Path, args: &[&str]) -> Result<String> {
    git_command(repository, args).map(|output| output.trim().to_owned())
}

fn git_must_fail(repository: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repository)
        .output()
        .with_context(|| format!("run git {}", args.join(" ")))?;
    if output.status.success() {
        bail!("git {} unexpectedly succeeded", args.join(" "));
    }
    Ok(())
}

fn git_command(repository: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repository)
        .output()
        .with_context(|| format!("run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
