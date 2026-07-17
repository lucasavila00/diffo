use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use diffo_app::{Model, update};
use diffo_core::{AccessMode, ChangeKind, Repository, RepositorySource};
use diffo_git::GitRepositorySource;
use diffo_tui::{ProgrammaticInputQueue, Renderer};
use diffo_watch::{RefreshResult, RefreshService};
use ratatui::layout::Rect;

const AREA: Rect = Rect::new(0, 0, 100, 30);

#[test]
fn space_stages_selected_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    let mut driver = Driver::new(&repository.worktree)?;

    driver.keys(|queue| {
        queue.key(KeyCode::Char(' '));
    })?;

    let file = driver.file("tracked.txt")?;
    assert!(file.staged.is_some());
    assert!(file.unstaged.is_none());
    Ok(())
}

#[test]
fn space_unstages_selected_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    let mut driver = Driver::new(&repository.worktree)?;

    driver.keys(|queue| {
        queue.key(KeyCode::Char(' '));
    })?;

    let file = driver.file("tracked.txt")?;
    assert!(file.staged.is_none());
    assert!(file.unstaged.is_some());
    Ok(())
}

#[test]
fn a_stages_all_files() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    fs::write(repository.worktree.join("new.txt"), "new\n")?;
    let mut driver = Driver::new(&repository.worktree)?;

    driver.keys(|queue| {
        queue.key(KeyCode::Char('a'));
    })?;

    assert!(
        driver
            .model
            .snapshot
            .files
            .iter()
            .all(|file| file.staged.is_some())
    );
    Ok(())
}

#[test]
fn a_unstages_all_files() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    fs::write(repository.worktree.join("new.txt"), "new\n")?;
    git(&repository.worktree, &["add", "."])?;
    let mut driver = Driver::new(&repository.worktree)?;

    driver.keys(|queue| {
        queue.key(KeyCode::Char('a'));
    })?;

    assert!(
        driver
            .model
            .snapshot
            .files
            .iter()
            .all(|file| file.staged.is_none())
    );
    assert_eq!(driver.file("new.txt")?.kind, ChangeKind::Untracked);
    Ok(())
}

#[test]
fn plus_button_stages_clicked_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    let mut driver = Driver::new(&repository.worktree)?;

    driver.keys(|queue| {
        queue.mouse(left_click(22, 16));
    })?;

    assert!(driver.file("tracked.txt")?.staged.is_some());
    Ok(())
}

#[test]
fn minus_button_unstages_clicked_file() -> Result<()> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    git(&repository.worktree, &["add", "tracked.txt"])?;
    let mut driver = Driver::new(&repository.worktree)?;

    driver.keys(|queue| {
        queue.mouse(left_click(22, 1));
    })?;

    assert!(driver.file("tracked.txt")?.staged.is_none());
    Ok(())
}

#[test]
fn palette_search_runs_fetch() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut driver = Driver::new(&repository.worktree)?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;

    driver.keys(|queue| {
        queue
            .key(KeyCode::Char('1'))
            .text("fetch")
            .key(KeyCode::Enter);
    })?;

    assert_eq!(driver.model.snapshot.upstream.as_ref().unwrap().behind, 1);
    assert!(!repository.worktree.join("remote.txt").exists());
    Ok(())
}

#[test]
fn palette_search_runs_pull() -> Result<()> {
    let repository = TestRepository::new()?;
    let mut driver = Driver::new(&repository.worktree)?;
    repository.commit_remote("remote.txt", "remote\n", "Remote commit")?;

    driver.keys(|queue| {
        queue
            .key(KeyCode::Char('1'))
            .text("pull")
            .key(KeyCode::Enter);
    })?;

    assert!(repository.worktree.join("remote.txt").exists());
    assert_eq!(driver.model.snapshot.upstream.as_ref().unwrap().behind, 0);
    Ok(())
}

struct Driver {
    model: Model,
    renderer: Renderer,
    refresh: RefreshService,
    deadline: Instant,
}

impl Driver {
    fn new(worktree: &Path) -> Result<Self> {
        let source = Arc::new(GitRepositorySource::new(worktree));
        let snapshot = source.snapshot()?;
        let paths = source.watch_paths()?;
        let repository = source as Arc<dyn Repository>;
        Ok(Self {
            model: Model::new(snapshot, AccessMode::ReadWrite),
            renderer: Renderer::new(),
            refresh: RefreshService::start(repository, &paths)?,
            deadline: Instant::now() + Duration::from_secs(5),
        })
    }

    fn keys(&mut self, build: impl FnOnce(&mut ProgrammaticInputQueue)) -> Result<()> {
        let mut queue = ProgrammaticInputQueue::new();
        build(&mut queue);
        while !queue.is_empty() {
            if let Some(message) = queue.pop_message(&mut self.renderer, &self.model, AREA)
                && let Some(diffo_app::Effect::Repository(action)) =
                    update(&mut self.model, message)
            {
                self.refresh.apply(action);
                self.wait_for_snapshot()?;
            }
        }
        Ok(())
    }

    fn wait_for_snapshot(&mut self) -> Result<()> {
        while Instant::now() < self.deadline {
            match self.refresh.try_recv() {
                Ok(Some(RefreshResult::Snapshot { snapshot, .. })) => {
                    self.model.refresh(snapshot);
                    return Ok(());
                }
                Ok(Some(RefreshResult::Error { message, .. })) => bail!(message),
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(error) => bail!("refresh worker stopped: {error}"),
            }
        }
        bail!("Git operation test exceeded its five-second deadline")
    }

    fn file(&self, path: &str) -> Result<&diffo_core::FileState> {
        self.model
            .snapshot
            .files
            .iter()
            .find(|file| file.path == Path::new(path))
            .with_context(|| format!("snapshot has no {path}"))
    }
}

fn left_click(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
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

    fn commit_remote(&self, path: &str, contents: &str, message: &str) -> Result<()> {
        fs::write(self.seed.join(path), contents)?;
        git(&self.seed, &["add", path])?;
        git(&self.seed, &["commit", "-m", message])?;
        git(&self.seed, &["push", "origin", "HEAD"])
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
    Ok(())
}
