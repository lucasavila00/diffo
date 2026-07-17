use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::OnceLock,
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use diffo_core::RepositorySnapshot;
use tempfile::TempDir;

#[test]
fn clean_repository() -> Result<()> {
    let repo = TestRepository::new()?;

    insta::assert_ron_snapshot!(collect(&repo.worktree)?, @r###"
RepositorySnapshot(
  branch: BranchState(
    name: Some("main"),
  ),
  files: [],
  recent_commits: [
    Commit(
      id: "4808d4013094be7c3fb50d5089f8e72193967c32",
      summary: "Base commit",
    ),
  ],
  upstream: Some(UpstreamState(
    name: "origin/main",
    ahead: 0,
    behind: 0,
  )),
)
"###);
    Ok(())
}

#[test]
fn unborn_repository_with_untracked_file() -> Result<()> {
    let root = tempfile::tempdir().context("create test directory")?;
    git(root.path(), &["init", "--initial-branch=main"])?;
    write(&root.path().join("first file.txt"), "not committed\n")?;

    insta::assert_ron_snapshot!(collect(root.path())?, @r###"
RepositorySnapshot(
  branch: BranchState(
    name: Some("main"),
  ),
  files: [
    FileState(
      path: "first file.txt",
      old_path: None,
      kind: Untracked,
      staged: None,
      unstaged: None,
    ),
  ],
  recent_commits: [],
  upstream: None,
)
"###);
    Ok(())
}

#[test]
fn staged_add_modify_delete_and_rename() -> Result<()> {
    let repo = TestRepository::new()?;
    write(&repo.worktree.join("added.txt"), "added\n")?;
    append(&repo.worktree.join("tracked.txt"), "modified\n")?;
    git(&repo.worktree, &["rm", "deleted.txt"])?;
    git(&repo.worktree, &["mv", "renamed.txt", "moved.txt"])?;
    git(&repo.worktree, &["add", "added.txt", "tracked.txt"])?;

    insta::assert_ron_snapshot!(collect(&repo.worktree)?);
    Ok(())
}

#[test]
fn unstaged_modify_delete_and_untracked_paths() -> Result<()> {
    let repo = TestRepository::new()?;
    append(&repo.worktree.join("tracked.txt"), "modified\n")?;
    fs::remove_file(repo.worktree.join("deleted.txt")).context("delete tracked file")?;
    write(&repo.worktree.join("untracked.txt"), "untracked\n")?;
    write(&repo.worktree.join("path with spaces.txt"), "spaces\n")?;

    insta::assert_ron_snapshot!(collect(&repo.worktree)?);
    Ok(())
}

#[test]
fn same_file_has_staged_and_unstaged_changes() -> Result<()> {
    let repo = TestRepository::new()?;
    append(&repo.worktree.join("tracked.txt"), "staged\n")?;
    git(&repo.worktree, &["add", "tracked.txt"])?;
    append(&repo.worktree.join("tracked.txt"), "unstaged\n")?;

    insta::assert_ron_snapshot!(collect(&repo.worktree)?);
    Ok(())
}

#[test]
fn local_branch_is_ahead_and_behind_upstream() -> Result<()> {
    let repo = TestRepository::new()?;
    repo.commit_local("local.txt", "local\n", "Local commit")?;
    repo.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    git(&repo.worktree, &["fetch", "origin"])?;

    insta::assert_ron_snapshot!(collect(&repo.worktree)?, @r###"
RepositorySnapshot(
  branch: BranchState(
    name: Some("main"),
  ),
  files: [],
  recent_commits: [
    Commit(
      id: "43d9e3e5b952193a00febda8eabb8fda9b45306f",
      summary: "Local commit",
    ),
    Commit(
      id: "4808d4013094be7c3fb50d5089f8e72193967c32",
      summary: "Base commit",
    ),
  ],
  upstream: Some(UpstreamState(
    name: "origin/main",
    ahead: 1,
    behind: 1,
  )),
)
"###);
    Ok(())
}

#[test]
fn local_branch_is_ahead_of_upstream() -> Result<()> {
    let repo = TestRepository::new()?;
    repo.commit_local("local.txt", "local\n", "Local commit")?;

    insta::assert_ron_snapshot!(collect(&repo.worktree)?, @r###"
RepositorySnapshot(
  branch: BranchState(
    name: Some("main"),
  ),
  files: [],
  recent_commits: [
    Commit(
      id: "43d9e3e5b952193a00febda8eabb8fda9b45306f",
      summary: "Local commit",
    ),
    Commit(
      id: "4808d4013094be7c3fb50d5089f8e72193967c32",
      summary: "Base commit",
    ),
  ],
  upstream: Some(UpstreamState(
    name: "origin/main",
    ahead: 1,
    behind: 0,
  )),
)
"###);
    Ok(())
}

#[test]
fn local_branch_is_behind_upstream() -> Result<()> {
    let repo = TestRepository::new()?;
    repo.commit_remote("remote.txt", "remote\n", "Remote commit")?;
    git(&repo.worktree, &["fetch", "origin"])?;

    insta::assert_ron_snapshot!(collect(&repo.worktree)?, @r###"
RepositorySnapshot(
  branch: BranchState(
    name: Some("main"),
  ),
  files: [],
  recent_commits: [
    Commit(
      id: "4808d4013094be7c3fb50d5089f8e72193967c32",
      summary: "Base commit",
    ),
  ],
  upstream: Some(UpstreamState(
    name: "origin/main",
    ahead: 0,
    behind: 1,
  )),
)
"###);
    Ok(())
}

#[test]
fn detached_head() -> Result<()> {
    let repo = TestRepository::new()?;
    git(&repo.worktree, &["checkout", "--detach"])?;

    insta::assert_ron_snapshot!(collect(&repo.worktree)?, @r###"
RepositorySnapshot(
  branch: BranchState(
    name: None,
  ),
  files: [],
  recent_commits: [
    Commit(
      id: "4808d4013094be7c3fb50d5089f8e72193967c32",
      summary: "Base commit",
    ),
  ],
  upstream: None,
)
"###);
    Ok(())
}

#[test]
fn conflicted_file() -> Result<()> {
    let repo = TestRepository::new()?;
    write(&repo.worktree.join("tracked.txt"), "local version\n")?;
    git(&repo.worktree, &["add", "tracked.txt"])?;
    git(&repo.worktree, &["commit", "-m", "Local edit"])?;

    repo.commit_remote("tracked.txt", "remote version\n", "Remote edit")?;
    git(&repo.worktree, &["fetch", "origin"])?;
    git_must_fail(&repo.worktree, &["merge", "origin/main"])?;

    insta::assert_ron_snapshot!(collect(&repo.worktree)?);
    Ok(())
}

#[test]
fn staged_binary_file() -> Result<()> {
    let repo = TestRepository::new()?;
    fs::write(repo.worktree.join("image.bin"), [0, 1, 2, 0, 255]).context("write binary file")?;
    git(&repo.worktree, &["add", "image.bin"])?;

    insta::assert_ron_snapshot!(collect(&repo.worktree)?);
    Ok(())
}

#[test]
fn ignored_file_is_not_reported() -> Result<()> {
    let repo = TestRepository::new()?;
    write(&repo.worktree.join(".gitignore"), "ignored.log\n")?;
    git(&repo.worktree, &["add", ".gitignore"])?;
    git(&repo.worktree, &["commit", "-m", "Add ignore rule"])?;
    write(&repo.worktree.join("ignored.log"), "ignored\n")?;

    insta::assert_ron_snapshot!(collect(&repo.worktree)?, @r###"
RepositorySnapshot(
  branch: BranchState(
    name: Some("main"),
  ),
  files: [],
  recent_commits: [
    Commit(
      id: "2f2f621b6b19ec8f7a241b03b37eeb2c1c3cb2bd",
      summary: "Add ignore rule",
    ),
    Commit(
      id: "4808d4013094be7c3fb50d5089f8e72193967c32",
      summary: "Base commit",
    ),
  ],
  upstream: Some(UpstreamState(
    name: "origin/main",
    ahead: 1,
    behind: 0,
  )),
)
"###);
    Ok(())
}

fn collect(repo: &Path) -> Result<RepositorySnapshot> {
    let dump_dir = tempfile::tempdir().context("create snapshot directory")?;
    let dump_path = dump_dir.path().join("repository-state.ron");
    let output = Command::new(diffo_binary()?)
        .current_dir(repo)
        .env("DIFFO_DUMP_PATH", &dump_path)
        .output()
        .context("run Diffo")?;
    if !output.status.success() {
        bail!(
            "Diffo failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let contents = fs::read_to_string(&dump_path).context("read Diffo state dump")?;
    ron::from_str(&contents).context("parse Diffo state dump")
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

struct TestRepository {
    _root: TempDir,
    seed: PathBuf,
    worktree: PathBuf,
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
        write(&seed.join("tracked.txt"), "base\n")?;
        write(&seed.join("deleted.txt"), "delete me\n")?;
        write(&seed.join("renamed.txt"), "rename me\n")?;
        git(&seed, &["add", "."])?;
        git(&seed, &["commit", "-m", "Base commit"])?;
        git(&seed, &["push", "-u", "origin", "HEAD"])?;

        git(root.path(), &["clone", "origin.git", "work"])?;
        let worktree = root.path().join("work");
        configure_author(&worktree)?;

        Ok(Self {
            _root: root,
            seed,
            worktree,
        })
    }

    fn commit_local(&self, path: &str, contents: &str, message: &str) -> Result<()> {
        write(&self.worktree.join(path), contents)?;
        git(&self.worktree, &["add", path])?;
        git(&self.worktree, &["commit", "-m", message])
    }

    fn commit_remote(&self, path: &str, contents: &str, message: &str) -> Result<()> {
        write(&self.seed.join(path), contents)?;
        git(&self.seed, &["add", path])?;
        git(&self.seed, &["commit", "-m", message])?;
        git(&self.seed, &["push", "origin", "HEAD"])
    }
}

fn configure_author(repo: &Path) -> Result<()> {
    git(repo, &["config", "user.name", "Diffo Test"])?;
    git(repo, &["config", "user.email", "diffo@example.invalid"])
}

fn write(path: &Path, text: &str) -> Result<()> {
    fs::write(path, text).with_context(|| format!("write {}", path.display()))
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
    let output = git_output(repo, args)?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn git_must_fail(repo: &Path, args: &[&str]) -> Result<()> {
    let output = git_output(repo, args)?;
    ensure!(
        !output.status.success(),
        "git {} unexpectedly succeeded",
        args.join(" ")
    );
    Ok(())
}

fn git_output(repo: &Path, args: &[&str]) -> Result<Output> {
    Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z")
        .output()
        .with_context(|| format!("run git {}", args.join(" ")))
}
