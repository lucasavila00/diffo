pub(super) use std::{
    ffi::OsStr,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

pub(super) use anyhow::{Context, Result, bail};
pub(super) use diffo_e2e::{DiffoScreen, Key, ScrollDirection, Selector};
pub(super) use serde::Deserialize;

pub(super) const TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const METADATA_LOOKING_SOURCE_BEFORE: &str = "before\nGIT binary patch\nBinary files a/x and b/x differ\ndiff --cc file.rs\n@@@ -1 -1 +1 @@@\n<<<<<<< HEAD\n=======\n>>>>>>> branch\n";
pub(super) const METADATA_LOOKING_SOURCE_AFTER: &str = "after\nGIT binary patch\nBinary files a/x and b/x differ\ndiff --cc file.rs\n@@@ -1 -1 +1 @@@\n<<<<<<< HEAD\n=======\n>>>>>>> branch\n";

pub(super) fn diffo_binary() -> Result<PathBuf> {
    diffo_e2e::diffo_binary(env!("CARGO_BIN_EXE_diffo"))
}

pub(super) fn numbered_lines(count: usize, change_neighbor: bool) -> Result<String> {
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

pub(super) fn large_file(prefix: &str, replacement: Option<&str>) -> Result<String> {
    let mut contents = String::new();
    for line in 0..650 {
        if line == 550 {
            writeln!(contents, "{}", replacement.unwrap_or(prefix))
                .context("build large changed line")?;
        } else {
            writeln!(contents, "{prefix} line {line:03}").context("build large file")?;
        }
    }
    Ok(contents)
}

pub(super) fn navigation_file(changed: bool) -> Result<String> {
    let mut contents = String::new();
    for line in 0..240 {
        let value = match (changed, line) {
            (true, 10) => "FIRST_CHANGE".to_owned(),
            (true, 120) => "MIDDLE_CHANGE".to_owned(),
            (true, 230) => "LAST_CHANGE".to_owned(),
            _ => format!("value_{line:03}"),
        };
        writeln!(contents, "pub const LINE_{line:03}: &str = \"{value}\";")
            .context("build hunk navigation file")?;
    }
    Ok(contents)
}

pub(super) fn large_syntax_file(changed: bool) -> Result<String> {
    let mut contents = String::new();
    for line in 1..10_000 {
        if changed && line == 9_000 {
            writeln!(contents, "pub const PERF_TARGET_09000: usize = 0;")
                .context("build syntax target")?;
        } else {
            writeln!(contents, "pub const LINE_{line:05}: usize = {line};")
                .context("build large syntax file")?;
        }
    }
    Ok(contents)
}

#[derive(Deserialize)]
pub(super) struct ScrollFrame {
    pub(super) input_events: Vec<String>,
    pub(super) scroll_before: (usize, usize),
    pub(super) scroll_after: (usize, usize),
    pub(super) event_read_us: Option<u64>,
    pub(super) draw_end_us: u64,
}

#[derive(Deserialize)]
pub(super) struct BufferFrame {
    pub(super) input_events: Vec<String>,
    pub(super) requested_diff: Option<String>,
    pub(super) displayed_diff: Option<String>,
    pub(super) viewport_transition: Option<(usize, usize)>,
    pub(super) scroll_before: (usize, usize),
    pub(super) scroll_after: (usize, usize),
    pub(super) first_rendered_row: usize,
    pub(super) syntax_ready: bool,
}

pub(super) fn changed_repository() -> Result<TestRepository> {
    let repository = TestRepository::new()?;
    fs::write(repository.worktree.join("tracked.txt"), "changed\n")?;
    fs::write(repository.worktree.join("new.txt"), "new\n")?;
    Ok(repository)
}

pub(super) fn all_changes_are_staged(repository: &Path) -> Result<bool> {
    let paths = cached_paths(repository)?;
    Ok(paths.contains("tracked.txt") && paths.contains("new.txt"))
}

pub(super) fn cached_paths(repository: &Path) -> Result<String> {
    git_output(repository, &["diff", "--cached", "--name-only"])
}

pub(super) fn wait_for(
    description: &str,
    mut condition: impl FnMut() -> Result<bool>,
) -> Result<()> {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        if condition()? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
    bail!("timed out waiting for {description}")
}

pub(super) fn confirm_protected_push(
    screen: &mut DiffoScreen,
    commits: usize,
    destination: &str,
) -> Result<()> {
    let noun = if commits == 1 { "commit" } else { "commits" };
    screen
        .wait_for_text("Confirm push")?
        .wait_for_text(&format!("Push {commits} {noun} directly to {destination}?"))?
        .wait_for_text("This bypasses the branch and pull-request workflow.")?
        .press(Key::Right)?
        .press(Key::Enter)?
        .wait_for_text_gone("Confirm push")?;
    Ok(())
}

pub(super) struct TestRepository {
    pub(super) root: tempfile::TempDir,
    seed: PathBuf,
    pub(super) worktree: PathBuf,
}

impl TestRepository {
    pub(super) fn new() -> Result<Self> {
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

    pub(super) fn screen(&self) -> Result<DiffoScreen> {
        DiffoScreen::launch(diffo_binary()?, &self.worktree)
    }

    pub(super) fn commit_remote(
        &self,
        path: &str,
        contents: &str,
        message: &str,
    ) -> Result<String> {
        fs::write(self.seed.join(path), contents)?;
        git(&self.seed, &["add", path])?;
        git(&self.seed, &["commit", "-m", message])?;
        git(&self.seed, &["push", "origin", "HEAD"])?;
        git_output(&self.seed, &["rev-parse", "HEAD"])
    }
}

pub(super) fn configure(repository: &Path) -> Result<()> {
    git(repository, &["config", "user.name", "Diffo Test"])?;
    git(
        repository,
        &["config", "user.email", "diffo@example.invalid"],
    )
}

pub(super) fn git(repository: &Path, args: &[&str]) -> Result<()> {
    git_command(repository, args).map(|_| ())
}

pub(super) fn git_output(repository: &Path, args: &[&str]) -> Result<String> {
    git_command(repository, args).map(|output| output.trim().to_owned())
}

pub(super) fn git_must_fail(repository: &Path, args: &[&str]) -> Result<()> {
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

pub(super) fn git_command(repository: &Path, args: &[&str]) -> Result<String> {
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
