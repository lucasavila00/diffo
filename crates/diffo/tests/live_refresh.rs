#![cfg(unix)]

use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use diffo_app::Model;
use diffo_core::{Repository, RepositorySnapshot, RepositorySource};
use diffo_git::GitRepositorySource;
use diffo_repository_service::{RepositoryEvent, RepositoryService};

#[test]
fn compiled_binary_refreshes_live_git_state() -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let repository = tempfile::tempdir().context("create repository")?;
    git(repository.path(), &["init", "--initial-branch=main"])?;
    git(repository.path(), &["config", "user.name", "Diffo Test"])?;
    git(
        repository.path(),
        &["config", "user.email", "diffo@example.invalid"],
    )?;
    fs::write(repository.path().join("tracked.txt"), "base\n")?;
    git(repository.path(), &["add", "."])?;
    git(repository.path(), &["commit", "-m", "Base commit"])?;

    let output = tempfile::tempdir().context("create dump directory")?;
    let dump = output.path().join("live.ron");
    let child = Command::new(env!("CARGO_BIN_EXE_diffo"))
        .current_dir(repository.path())
        .env("DIFFO_WATCH_DUMP_PATH", &dump)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .context("start Diffo watch mode")?;
    let mut child = ChildGuard(Some(child));

    wait_for(&dump, deadline, "initial snapshot", |snapshot| {
        snapshot.files.is_empty()
    })?;
    fs::write(repository.path().join("tracked.txt"), "changed\n")?;
    fs::write(repository.path().join("new.txt"), "new\n")?;
    wait_for(&dump, deadline, "unstaged changes", |snapshot| {
        snapshot.files.len() == 2 && snapshot.files.iter().all(|file| file.staged.is_none())
    })?;

    git(repository.path(), &["add", "."])?;
    wait_for(&dump, deadline, "staged changes", |snapshot| {
        snapshot.files.len() == 2 && snapshot.files.iter().all(|file| file.staged.is_some())
    })?;

    git(repository.path(), &["commit", "-m", "Live commit"])?;
    wait_for(&dump, deadline, "committed changes", |snapshot| {
        snapshot.files.is_empty()
            && snapshot
                .recent_commits
                .first()
                .is_some_and(|commit| commit.summary == "Live commit")
    })?;

    let process = child.0.as_mut().expect("child is running");
    let status = Command::new("kill")
        .args(["-TERM", &process.id().to_string()])
        .status()
        .context("send SIGTERM")?;
    if !status.success() {
        bail!("failed to send SIGTERM");
    }
    let status = wait_for_child(process, deadline)?;
    child.0.take();
    if !status.success() {
        bail!("Diffo did not shut down cleanly: {status}");
    }
    Ok(())
}

#[test]
fn watcher_refresh_never_resets_scroll_for_the_same_selection() -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let repository = tempfile::tempdir().context("create repository")?;
    git(repository.path(), &["init", "--initial-branch=main"])?;
    git(repository.path(), &["config", "user.name", "Diffo Test"])?;
    git(
        repository.path(),
        &["config", "user.email", "diffo@example.invalid"],
    )?;
    fs::write(repository.path().join(".gitignore"), "ignored.tmp\n")?;
    fs::write(repository.path().join("tracked.txt"), "base\n")?;
    git(repository.path(), &["add", "."])?;
    git(repository.path(), &["commit", "-m", "Base commit"])?;
    fs::write(repository.path().join("tracked.txt"), "first change\n")?;

    let source = Arc::new(GitRepositorySource::new(repository.path()));
    let paths = source.watch_paths()?;
    let snapshot = source.snapshot()?;
    let repository_source = Arc::clone(&source) as Arc<dyn Repository>;
    let repository_service = RepositoryService::start(repository_source, Some(&paths))?;
    let mut model = Model::new(snapshot);
    model.diff_scroll = 40;

    fs::write(repository.path().join("ignored.tmp"), "ignored\n")?;
    model.repository_changed(wait_for_snapshot(&repository_service, deadline, |_| true)?);
    assert_eq!(model.diff_scroll, 40);

    fs::write(repository.path().join("tracked.txt"), "second change\n")?;
    model.repository_changed(wait_for_snapshot(
        &repository_service,
        deadline,
        |snapshot| {
            snapshot.files.iter().any(|file| {
                file.unstaged
                    .as_ref()
                    .is_some_and(|diff| diff.text.contains("second change"))
            })
        },
    )?);
    assert_eq!(model.diff_scroll, 40);
    Ok(())
}

fn wait_for_snapshot(
    repository_service: &RepositoryService,
    deadline: Instant,
    predicate: impl Fn(&RepositorySnapshot) -> bool,
) -> Result<RepositorySnapshot> {
    while Instant::now() < deadline {
        match repository_service.try_recv() {
            Ok(Some(RepositoryEvent::SnapshotRefreshed { snapshot, .. }))
                if predicate(&snapshot) =>
            {
                return Ok(snapshot);
            }
            Ok(Some(
                RepositoryEvent::Prompt { .. }
                | RepositoryEvent::SnapshotRefreshed { .. }
                | RepositoryEvent::CommandCompleted { .. }
                | RepositoryEvent::CommandFailed { .. }
                | RepositoryEvent::CommandCancelled { .. },
            )) => {}
            Ok(Some(RepositoryEvent::RefreshFailed { message, .. })) => bail!(message),
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => bail!("repository service stopped: {error}"),
        }
    }
    bail!("watcher scroll regression test exceeded its 5-second deadline")
}

fn wait_for(
    path: &Path,
    deadline: Instant,
    state: &str,
    predicate: impl Fn(&RepositorySnapshot) -> bool,
) -> Result<()> {
    while Instant::now() < deadline {
        if let Ok(contents) = fs::read_to_string(path)
            && let Ok(snapshot) = ron::from_str::<RepositorySnapshot>(&contents)
            && predicate(&snapshot)
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(20));
    }
    bail!("test exceeded its 10-second deadline waiting for {state}")
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

fn wait_for_child(child: &mut Child, deadline: Instant) -> Result<std::process::ExitStatus> {
    loop {
        if let Some(status) = child.try_wait().context("poll child process")? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("test exceeded its 10-second deadline");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

struct ChildGuard(Option<Child>);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
