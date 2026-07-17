use std::{path::PathBuf, process::Command};

use anyhow::{Context, Result, bail};

use crate::repository::{
    BranchState, ChangeKind, Commit, FileDiff, FileState, RepositorySnapshot, RepositorySource,
    UpstreamState,
};

const NO_CHANGE: char = '.';

pub struct GitRepositorySource {
    root: PathBuf,
}

impl GitRepositorySource {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
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

    fn diff(&self, path: &str, staged: bool) -> Result<Option<FileDiff>> {
        let mut args = vec!["diff", "--no-ext-diff", "--no-color"];
        if staged {
            args.push("--cached");
        }
        args.extend(["--", path]);

        let text = String::from_utf8(self.git(&args)?)
            .with_context(|| format!("git returned a non-UTF-8 diff for {path}"))?;
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
            let staged = if file.index_status == NO_CHANGE {
                None
            } else {
                self.diff(&path, true)?
            };
            let unstaged = if file.worktree_status == NO_CHANGE {
                None
            } else {
                self.diff(&path, false)?
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
    use std::path::PathBuf;

    use super::parse_status;
    use crate::repository::ChangeKind;

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
}
