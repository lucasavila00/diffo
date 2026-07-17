use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::Command,
};

use anyhow::{Context, Result, bail};
use diffo_core::RepositorySource;
use diffo_git::GitRepositorySource;
use tempfile::TempDir;

#[test]
fn collects_complete_repository_state() -> Result<()> {
    let test_repo = TestRepository::new()?;
    test_repo.create_unpushed_commit()?;
    test_repo.create_staged_and_unstaged_change()?;
    test_repo.create_untracked_file()?;

    let snapshot = GitRepositorySource::new(&test_repo.worktree).snapshot()?;

    insta::assert_ron_snapshot!(snapshot);
    Ok(())
}

struct TestRepository {
    _root: TempDir,
    worktree: std::path::PathBuf,
}

impl TestRepository {
    fn new() -> Result<Self> {
        let root = tempfile::tempdir().context("create test directory")?;
        git(
            root.path(),
            &["init", "--bare", "--initial-branch=main", "origin.git"],
        )?;
        git(root.path(), &["clone", "origin.git", "seed"])?;

        let seed = root.path().join("seed");
        configure_author(&seed)?;
        fs::write(seed.join("tracked.txt"), "base\n").context("write base file")?;
        git(&seed, &["add", "tracked.txt"])?;
        git(&seed, &["commit", "-m", "Base commit"])?;
        git(&seed, &["push", "-u", "origin", "HEAD"])?;

        git(root.path(), &["clone", "origin.git", "work"])?;
        let worktree = root.path().join("work");
        configure_author(&worktree)?;

        Ok(Self {
            _root: root,
            worktree,
        })
    }

    fn create_unpushed_commit(&self) -> Result<()> {
        fs::write(self.worktree.join("committed.txt"), "committed\n")
            .context("write committed file")?;
        git(&self.worktree, &["add", "committed.txt"])?;
        git(&self.worktree, &["commit", "-m", "Unpushed commit"])
    }

    fn create_staged_and_unstaged_change(&self) -> Result<()> {
        append(&self.worktree.join("tracked.txt"), "staged\n")?;
        git(&self.worktree, &["add", "tracked.txt"])?;
        append(&self.worktree.join("tracked.txt"), "unstaged\n")
    }

    fn create_untracked_file(&self) -> Result<()> {
        fs::write(self.worktree.join("untracked.txt"), "untracked\n")
            .context("write untracked file")
    }
}

fn configure_author(repo: &Path) -> Result<()> {
    git(repo, &["config", "user.name", "Diffo Test"])?;
    git(repo, &["config", "user.email", "diffo@example.invalid"])
}

fn append(path: &Path, text: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    file.write_all(text.as_bytes())
        .with_context(|| format!("append to {}", path.display()))
}

fn git(repo: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z")
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
