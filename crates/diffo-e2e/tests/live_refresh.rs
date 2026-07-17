#![cfg(unix)]

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use diffo_core::RepositorySnapshot;

#[test]
fn compiled_binary_refreshes_live_git_state() -> Result<()> {
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
    let child = Command::new(diffo_binary()?)
        .current_dir(repository.path())
        .env("DIFFO_WATCH_DUMP_PATH", &dump)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("start Diffo watch mode")?;
    let mut child = ChildGuard(Some(child));

    wait_for(&dump, |snapshot| snapshot.files.is_empty())?;
    fs::write(repository.path().join("tracked.txt"), "changed\n")?;
    fs::write(repository.path().join("new.txt"), "new\n")?;
    wait_for(&dump, |snapshot| {
        snapshot.files.len() == 2 && snapshot.files.iter().all(|file| file.staged.is_none())
    })?;

    git(repository.path(), &["add", "."])?;
    wait_for(&dump, |snapshot| {
        snapshot.files.len() == 2 && snapshot.files.iter().all(|file| file.staged.is_some())
    })?;

    git(repository.path(), &["commit", "-m", "Live commit"])?;
    wait_for(&dump, |snapshot| {
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
    let status = process.wait().context("wait for Diffo")?;
    child.0.take();
    if !status.success() {
        bail!("Diffo did not shut down cleanly: {status}");
    }
    Ok(())
}

fn wait_for(path: &Path, predicate: impl Fn(&RepositorySnapshot) -> bool) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(contents) = fs::read_to_string(path)
            && let Ok(snapshot) = ron::from_str::<RepositorySnapshot>(&contents)
            && predicate(&snapshot)
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(20));
    }
    bail!("timed out waiting for live snapshot at {}", path.display())
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

fn diffo_binary() -> Result<PathBuf> {
    static BINARY: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    BINARY
        .get_or_init(build_diffo)
        .as_ref()
        .cloned()
        .map_err(|error| anyhow!(error))
}

fn build_diffo() -> Result<PathBuf, String> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "cannot find workspace root".to_owned())?;
    let target = workspace.join("target").join("diffo-e2e-cli");
    let output = Command::new("cargo")
        .args(["build", "--quiet", "--package", "diffo", "--target-dir"])
        .arg(&target)
        .current_dir(workspace)
        .output()
        .map_err(|error| format!("failed to build Diffo: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "failed to build Diffo: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(target
        .join("debug")
        .join(format!("diffo{}", std::env::consts::EXE_SUFFIX)))
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
