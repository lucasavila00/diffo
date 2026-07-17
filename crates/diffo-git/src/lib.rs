use std::{path::PathBuf, process::Command};

use anyhow::{Context, Result, bail};

use diffo_core::{
    AccessMode, BranchState, ChangeKind, Commit, FileDiff, FileState, Repository, RepositoryAction,
    RepositorySnapshot, RepositorySource, UpstreamState,
};

const NO_CHANGE: char = '.';

pub struct GitRepositorySource {
    root: PathBuf,
    access_mode: AccessMode,
}

impl GitRepositorySource {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            access_mode: AccessMode::ReadWrite,
        }
    }

    #[must_use]
    pub fn with_access_mode(mut self, access_mode: AccessMode) -> Self {
        self.access_mode = access_mode;
        self
    }

    fn git(&self, args: &[&str]) -> Result<Vec<u8>> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()
            .context("failed to run git; is it installed and available on PATH?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git {} failed: {}", args.join(" "), stderr.trim());
        }

        Ok(output.stdout)
    }

    fn diff(&self, paths: &[&str], staged: bool) -> Result<Option<FileDiff>> {
        let mut args = vec!["diff", "--no-ext-diff", "--no-color"];
        if staged {
            args.push("--cached");
        }
        args.push("--");
        args.extend(paths);

        let text = String::from_utf8(self.git(&args)?)
            .with_context(|| format!("git returned a non-UTF-8 diff for {}", paths.join(", ")))?;
        Ok((!text.is_empty()).then_some(FileDiff { text }))
    }

    fn recent_commits(&self) -> Result<Vec<Commit>> {
        let has_head = Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&self.root)
            .output()
            .context("failed to check Git HEAD")?
            .status
            .success();
        if !has_head {
            return Ok(Vec::new());
        }

        let output = String::from_utf8(self.git(&["log", "-n", "50", "--format=%H%x00%s%x00"])?)
            .context("git returned a non-UTF-8 commit log")?;
        let fields = output.split('\0').collect::<Vec<_>>();

        Ok(fields
            .chunks(2)
            .filter_map(|fields| match fields {
                [id, summary] if !id.trim().is_empty() => Some(Commit {
                    id: id.trim().to_owned(),
                    summary: (*summary).to_owned(),
                }),
                _ => None,
            })
            .collect())
    }
}

impl Default for GitRepositorySource {
    fn default() -> Self {
        Self::new(".")
    }
}

impl RepositorySource for GitRepositorySource {
    fn snapshot(&self) -> Result<RepositorySnapshot> {
        let status = self.git(&["status", "--porcelain=v2", "--branch", "-z"])?;
        let parsed = parse_status(&status)?;
        let mut files = Vec::with_capacity(parsed.files.len());

        for file in parsed.files {
            let path = file.state.path.to_string_lossy();
            let old_path = file
                .state
                .old_path
                .as_ref()
                .map(|path| path.to_string_lossy());
            let paths = old_path
                .as_deref()
                .map_or_else(|| vec![path.as_ref()], |old| vec![old, path.as_ref()]);
            let staged = if file.index_status == NO_CHANGE {
                None
            } else {
                self.diff(&paths, true)?
            };
            let unstaged = if file.worktree_status == NO_CHANGE {
                None
            } else {
                self.diff(&paths, false)?
            };
            files.push(FileState {
                staged,
                unstaged,
                ..file.state
            });
        }

        Ok(RepositorySnapshot {
            branch: parsed.branch,
            files,
            recent_commits: self.recent_commits()?,
            upstream: parsed.upstream,
        })
    }
}

impl Repository for GitRepositorySource {
    fn access_mode(&self) -> AccessMode {
        self.access_mode
    }

    fn apply(&self, action: &RepositoryAction) -> Result<()> {
        if self.access_mode == AccessMode::ReadOnly {
            bail!("repository is read-only");
        }

        let mut command = Command::new("git");
        command.current_dir(&self.root);
        match action {
            RepositoryAction::Stage(path) => {
                command.args(["add", "--"]).arg(path);
            }
            RepositoryAction::Unstage(path) => {
                command.args(["reset", "--"]).arg(path);
            }
            RepositoryAction::StageAll => {
                command.args(["add", "--all"]);
            }
            RepositoryAction::UnstageAll => {
                command.arg("reset");
            }
        }

        let output = command.output().context("failed to run git index action")?;
        if !output.status.success() {
            bail!(
                "git index action failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }
}

struct ParsedStatus {
    branch: BranchState,
    files: Vec<ParsedFile>,
    upstream: Option<UpstreamState>,
}

struct ParsedFile {
    state: FileState,
    index_status: char,
    worktree_status: char,
}

fn parse_status(output: &[u8]) -> Result<ParsedStatus> {
    let records = output.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut branch = BranchState::default();
    let mut upstream_name = None;
    let mut ahead = 0;
    let mut behind = 0;
    let mut files = Vec::new();
    let mut index = 0;

    while index < records.len() {
        let record = std::str::from_utf8(records[index]).context("git status was not UTF-8")?;
        index += 1;
        if record.is_empty() {
            continue;
        }

        if let Some(name) = record.strip_prefix("# branch.head ") {
            branch.name = (name != "(detached)").then(|| name.to_owned());
        } else if let Some(name) = record.strip_prefix("# branch.upstream ") {
            upstream_name = Some(name.to_owned());
        } else if let Some(counts) = record.strip_prefix("# branch.ab ") {
            let mut parts = counts.split_whitespace();
            ahead = parse_count(parts.next(), '+')?;
            behind = parse_count(parts.next(), '-')?;
        } else if record.starts_with("1 ") {
            let fields = record.splitn(9, ' ').collect::<Vec<_>>();
            let [_, xy, _, _, _, _, _, _, path] = fields.as_slice() else {
                bail!("invalid ordinary git status record: {record}");
            };
            files.push(parsed_file(path, None, xy)?);
        } else if record.starts_with("2 ") {
            let fields = record.splitn(10, ' ').collect::<Vec<_>>();
            let [_, xy, _, _, _, _, _, _, _, path] = fields.as_slice() else {
                bail!("invalid renamed git status record: {record}");
            };
            let old_path = records
                .get(index)
                .context("rename record has no old path")?;
            index += 1;
            let old_path = std::str::from_utf8(old_path).context("old path was not UTF-8")?;
            files.push(parsed_file(path, Some(old_path), xy)?);
        } else if record.starts_with("u ") {
            let fields = record.splitn(11, ' ').collect::<Vec<_>>();
            let path = fields.last().context("conflict record has no path")?;
            files.push(ParsedFile {
                state: empty_file(path, None, ChangeKind::Conflicted),
                index_status: 'U',
                worktree_status: 'U',
            });
        } else if let Some(path) = record.strip_prefix("? ") {
            files.push(ParsedFile {
                state: empty_file(path, None, ChangeKind::Untracked),
                index_status: NO_CHANGE,
                worktree_status: NO_CHANGE,
            });
        }
    }

    let upstream = upstream_name.map(|name| UpstreamState {
        name,
        ahead,
        behind,
    });
    Ok(ParsedStatus {
        branch,
        files,
        upstream,
    })
}

fn parse_count(value: Option<&str>, prefix: char) -> Result<usize> {
    let value = value.context("branch count is missing")?;
    value
        .strip_prefix(prefix)
        .context("branch count has an invalid prefix")?
        .parse()
        .context("branch count is not a number")
}

fn parsed_file(path: &str, old_path: Option<&str>, xy: &str) -> Result<ParsedFile> {
    let mut statuses = xy.chars();
    let index_status = statuses.next().context("index status is missing")?;
    let worktree_status = statuses.next().context("worktree status is missing")?;
    let status = if index_status == NO_CHANGE {
        worktree_status
    } else {
        index_status
    };
    Ok(ParsedFile {
        state: empty_file(path, old_path, change_kind(status)?),
        index_status,
        worktree_status,
    })
}

fn empty_file(path: &str, old_path: Option<&str>, kind: ChangeKind) -> FileState {
    FileState {
        path: PathBuf::from(path),
        old_path: old_path.map(PathBuf::from),
        kind,
        staged: None,
        unstaged: None,
    }
}

fn change_kind(status: char) -> Result<ChangeKind> {
    match status {
        'A' => Ok(ChangeKind::Added),
        'M' | 'T' => Ok(ChangeKind::Modified),
        'D' => Ok(ChangeKind::Deleted),
        'R' => Ok(ChangeKind::Renamed),
        'C' => Ok(ChangeKind::Copied),
        'U' => Ok(ChangeKind::Conflicted),
        _ => bail!("unknown git file status: {status}"),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
    };

    use super::parse_status;
    use diffo_core::{AccessMode, ChangeKind, Repository, RepositoryAction, RepositorySource};

    #[test]
    fn parses_branch_files_and_upstream() {
        let status = b"# branch.head feature\0# branch.upstream origin/feature\0# branch.ab +2 -1\x001 M. N... 100644 100644 100644 abc def file.txt\0? notes.txt\0";

        let parsed = parse_status(status).expect("status should parse");

        assert_eq!(parsed.branch.name.as_deref(), Some("feature"));
        assert_eq!(parsed.upstream.expect("upstream should exist").ahead, 2);
        assert_eq!(parsed.files.len(), 2);
        assert_eq!(parsed.files[0].state.path, PathBuf::from("file.txt"));
        assert_eq!(parsed.files[1].state.kind, ChangeKind::Untracked);
    }

    #[test]
    fn parses_rename_with_old_path() {
        let status = b"2 R. N... 100644 100644 100644 abc def R100 new.txt\0old.txt\0";

        let parsed = parse_status(status).expect("status should parse");

        assert_eq!(parsed.files[0].state.kind, ChangeKind::Renamed);
        assert_eq!(
            parsed.files[0].state.old_path,
            Some(PathBuf::from("old.txt"))
        );
    }

    #[test]
    fn stages_and_unstages_a_file() {
        let repo = test_repository();
        fs::write(repo.path().join("new.txt"), "new\n").expect("write file");
        let source = super::GitRepositorySource::new(repo.path());

        source
            .apply(&RepositoryAction::Stage(PathBuf::from("new.txt")))
            .expect("stage file");
        assert!(
            source
                .snapshot()
                .expect("staged snapshot")
                .files
                .iter()
                .any(|file| file.path == Path::new("new.txt") && file.staged.is_some())
        );

        source
            .apply(&RepositoryAction::Unstage(PathBuf::from("new.txt")))
            .expect("unstage file");
        let file = source
            .snapshot()
            .expect("unstaged snapshot")
            .files
            .into_iter()
            .find(|file| file.path == Path::new("new.txt"))
            .expect("new file");
        assert_eq!(file.kind, ChangeKind::Untracked);
        assert!(file.staged.is_none());
    }

    #[test]
    fn stages_and_unstages_all_files() {
        let repo = test_repository();
        fs::write(repo.path().join("tracked.txt"), "changed\n").expect("modify file");
        fs::write(repo.path().join("new.txt"), "new\n").expect("write file");
        let source = super::GitRepositorySource::new(repo.path());

        source
            .apply(&RepositoryAction::StageAll)
            .expect("stage all files");
        let snapshot = source.snapshot().expect("snapshot");

        assert_eq!(snapshot.files.len(), 2);
        assert!(snapshot.files.iter().all(|file| file.staged.is_some()));

        source
            .apply(&RepositoryAction::UnstageAll)
            .expect("unstage all files");
        let snapshot = source.snapshot().expect("unstaged snapshot");
        assert_eq!(snapshot.files.len(), 2);
        assert!(snapshot.files.iter().all(|file| file.staged.is_none()));
    }

    #[test]
    fn read_only_source_rejects_actions() {
        let repo = test_repository();
        let source =
            super::GitRepositorySource::new(repo.path()).with_access_mode(AccessMode::ReadOnly);

        let error = source
            .apply(&RepositoryAction::StageAll)
            .expect_err("read-only action should fail");

        assert!(error.to_string().contains("read-only"));
    }

    fn test_repository() -> tempfile::TempDir {
        let repo = tempfile::tempdir().expect("test directory");
        git(repo.path(), &["init", "--initial-branch=main"]);
        git(repo.path(), &["config", "user.name", "Diffo Test"]);
        git(
            repo.path(),
            &["config", "user.email", "diffo@example.invalid"],
        );
        fs::write(repo.path().join("tracked.txt"), "base\n").expect("write tracked file");
        git(repo.path(), &["add", "tracked.txt"]);
        git(repo.path(), &["commit", "-m", "Base commit"]);
        repo
    }

    fn git(repo: &std::path::Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("run git");
        assert!(
            status.status.success(),
            "git failed: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }
}
