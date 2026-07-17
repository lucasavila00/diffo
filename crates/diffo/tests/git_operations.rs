use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use diffo_e2e::{DiffoPage, Key, Selector};

const TIMEOUT: Duration = Duration::from_secs(5);

#[test]
fn space_stages_selected_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    let mut page = repository.page()?;

    page.press(Key::Char(' '))?;

    wait_for("tracked.txt to be staged", || {
        Ok(cached_paths(&repository.worktree)?.contains("tracked.txt"))
    })
}

#[test]
fn space_unstages_selected_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    let mut page = repository.page()?;

    page.press(Key::Char(' '))?;

    wait_for("tracked.txt to be unstaged", || {
        Ok(!cached_paths(&repository.worktree)?.contains("tracked.txt"))
    })
}

#[test]
fn a_stages_all_files() -> Result<()> {
    let repository = changed_repository()?;
    let mut page = repository.page()?;

    page.press(Key::Char('a'))?;

    wait_for("all files to be staged", || {
        all_changes_are_staged(&repository.worktree)
    })
}

#[test]
fn a_unstages_all_files() -> Result<()> {
    let repository = changed_repository()?;
    git(&repository.worktree, &["add", "."])?;
    let mut page = repository.page()?;

    page.press(Key::Char('a'))?;

    wait_for("all files to be unstaged", || {
        Ok(cached_paths(&repository.worktree)?.is_empty())
    })
}

#[test]
fn changes_header_stages_all_files() -> Result<()> {
    let repository = changed_repository()?;
    let mut page = repository.page()?;

    page.click(Selector::panel_action("Changes", "Stage All"))?;

    wait_for("header action to stage all files", || {
        all_changes_are_staged(&repository.worktree)
    })
}

#[test]
fn staged_header_unstages_all_files() -> Result<()> {
    let repository = changed_repository()?;
    git(&repository.worktree, &["add", "."])?;
    let mut page = repository.page()?;

    page.click(Selector::panel_action("Staged", "Unstage All"))?;

    wait_for("header action to unstage all files", || {
        Ok(cached_paths(&repository.worktree)?.is_empty())
    })
}

#[test]
fn plus_button_stages_clicked_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    let mut page = repository.page()?;

    page.click(Selector::file_action("Changes", "tracked.txt", "[+]"))?;

    wait_for("clicked file to be staged", || {
        Ok(cached_paths(&repository.worktree)?.contains("tracked.txt"))
    })
}

#[test]
fn minus_button_unstages_clicked_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    let mut page = repository.page()?;

    page.click(Selector::file_action("Staged", "tracked.txt", "[-]"))?;

    wait_for("clicked file to be unstaged", || {
        Ok(!cached_paths(&repository.worktree)?.contains("tracked.txt"))
    })
}

#[test]
fn palette_search_runs_fetch() -> Result<()> {
    let repository = TestRepository::new()?;
    let remote_commit = repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    let mut page = repository.page()?;

    page.press(Key::Char('1'))?
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
    let mut page = repository.page()?;

    page.press(Key::Char('1'))?
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
    let mut page = repository.page()?;

    page.press(Key::Char('1'))?
        .wait_for_text("Command Palette")?
        .type_text("pull")?
        .click(Selector::text("Git: Pull"))?;

    wait_for("clicked pull command to finish", || {
        Ok(repository.worktree.join("remote.txt").exists())
    })
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
    _root: tempfile::TempDir,
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
            _root: root,
            seed,
            worktree,
        })
    }

    fn page(&self) -> Result<DiffoPage> {
        DiffoPage::launch(env!("CARGO_BIN_EXE_diffo"), &self.worktree)
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
