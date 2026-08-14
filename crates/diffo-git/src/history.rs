use std::process::Command;

use anyhow::{Context, Result, bail};
use diffo_core::{CheckoutHistory, Commit};

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
        let first_parent = fields.next();
        let output = if let Some(parent) = first_parent {
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
}
