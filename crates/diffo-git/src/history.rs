use std::{path::Path, process::Command};

use anyhow::{Context, Result, bail};
use diffo_core::{ChangeKind, CheckoutHistory, Commit, CommitFile, CommitReview};

use super::GitRepositorySource;

impl GitRepositorySource {
    pub(super) fn checkout_commit_history(&self) -> Result<CheckoutHistory> {
        let has_head = Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&self.root)
            .output()
            .context("failed to check Git HEAD")?
            .status
            .success();
        if !has_head {
            return Ok(CheckoutHistory {
                head_commit: None,
                commits: Vec::new(),
            });
        }
        let head_commit = String::from_utf8(self.git(&["rev-parse", "--verify", "HEAD"])?)
            .context("git returned a non-UTF-8 HEAD commit")?;
        let head_commit = head_commit.trim().to_owned();

        let output = String::from_utf8(self.git(&[
            "log",
            "--topo-order",
            "--format=%H%x00%s%x00",
            "HEAD",
        ])?)
        .context("git returned a non-UTF-8 commit history")?;
        let fields = output.split('\0').collect::<Vec<_>>();
        let commits = fields
            .chunks(2)
            .filter_map(|fields| match fields {
                [id, summary] if !id.trim().is_empty() => Some(Commit {
                    id: id.trim().to_owned(),
                    summary: (*summary).to_owned(),
                }),
                _ => None,
            })
            .collect();
        Ok(CheckoutHistory {
            head_commit: Some(head_commit),
            commits,
        })
    }

    pub(super) fn recorded_commit_patch(&self, commit_id: &str) -> Result<String> {
        let first_parent = self.first_parent(commit_id)?;
        let output = if let Some(parent) = first_parent.as_deref() {
            self.git(&[
                "diff",
                "--no-ext-diff",
                "--no-color",
                "--find-renames",
                "--unified=3",
                parent,
                commit_id,
                "--",
            ])?
        } else {
            self.git(&[
                "show",
                "--format=",
                "--no-ext-diff",
                "--no-color",
                "--find-renames",
                "--unified=3",
                commit_id,
                "--",
            ])?
        };
        String::from_utf8(output).context("git returned a non-UTF-8 commit patch")
    }

    pub(super) fn recorded_commit_review(&self, commit_id: &str) -> Result<CommitReview> {
        Ok(CommitReview {
            patch: self.recorded_commit_patch(commit_id)?,
            files: self.recorded_commit_files(commit_id)?,
        })
    }

    fn recorded_commit_files(&self, commit_id: &str) -> Result<Vec<CommitFile>> {
        let first_parent = self.first_parent(commit_id)?;
        let output = if let Some(parent) = first_parent.as_deref() {
            self.git(&[
                "diff",
                "--name-status",
                "--find-renames",
                "-z",
                parent,
                commit_id,
                "--",
            ])?
        } else {
            self.git(&[
                "diff-tree",
                "--root",
                "--no-commit-id",
                "--name-status",
                "--find-renames",
                "-r",
                "-z",
                commit_id,
                "--",
            ])?
        };
        parse_commit_files(&output)
    }

    pub(super) fn recorded_commit_file_patch(
        &self,
        commit_id: &str,
        path: &Path,
        old_path: Option<&Path>,
    ) -> Result<String> {
        let first_parent = self.first_parent(commit_id)?;
        let path = path.to_string_lossy();
        let old_path = old_path.map(Path::to_string_lossy);
        let mut args = if let Some(parent) = first_parent.as_deref() {
            vec![
                "diff",
                "--no-ext-diff",
                "--no-color",
                "--find-renames",
                "--unified=2147483647",
                parent,
                commit_id,
                "--",
            ]
        } else {
            vec![
                "show",
                "--format=",
                "--no-ext-diff",
                "--no-color",
                "--find-renames",
                "--unified=2147483647",
                commit_id,
                "--",
            ]
        };
        if let Some(old_path) = old_path.as_deref()
            && old_path != path
        {
            args.push(old_path);
        }
        args.push(&path);
        let output = self.git(&args)?;
        if output.is_empty() {
            bail!("commit does not change {path}");
        }
        String::from_utf8(output).context("git returned a non-UTF-8 commit file patch")
    }

    fn first_parent(&self, commit_id: &str) -> Result<Option<String>> {
        if commit_id.is_empty() || !commit_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("commit id is invalid");
        }
        let parents =
            String::from_utf8(self.git(&["rev-list", "--parents", "-n", "1", commit_id])?)
                .context("git returned non-UTF-8 commit parents")?;
        let mut fields = parents.split_whitespace();
        let resolved = fields.next().context("git returned no commit")?;
        if resolved != commit_id {
            bail!("git resolved a different commit");
        }
        Ok(fields.next().map(str::to_owned))
    }
}

fn parse_commit_files(output: &[u8]) -> Result<Vec<CommitFile>> {
    let records = output.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut files = Vec::new();
    let mut index = 0;
    while index < records.len() {
        let status = std::str::from_utf8(records[index])
            .context("git returned a non-UTF-8 commit file status")?;
        index += 1;
        if status.is_empty() {
            continue;
        }
        let kind = match status.chars().next() {
            Some('A') => ChangeKind::Added,
            Some('M' | 'T') => ChangeKind::Modified,
            Some('D') => ChangeKind::Deleted,
            Some('R') => ChangeKind::Renamed,
            Some('C') => ChangeKind::Copied,
            _ => bail!("git returned an unknown commit file status: {status}"),
        };
        let first = records.get(index).context("commit file path is missing")?;
        index += 1;
        let first = std::str::from_utf8(first).context("commit file path was not UTF-8")?;
        let (old_path, path) = if matches!(kind, ChangeKind::Renamed | ChangeKind::Copied) {
            let second = records
                .get(index)
                .context("renamed commit file path is missing")?;
            index += 1;
            let second =
                std::str::from_utf8(second).context("renamed commit file path was not UTF-8")?;
            (
                Some(Path::new(first).to_path_buf()),
                Path::new(second).to_path_buf(),
            )
        } else {
            (None, Path::new(first).to_path_buf())
        };
        files.push(CommitFile {
            path,
            old_path,
            kind,
        });
    }
    Ok(files)
}
