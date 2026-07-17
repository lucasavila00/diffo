use std::{path::PathBuf, process::Command};

use anyhow::{Context, Result, bail};

use crate::repository::{ChangeKind, FileDiff, FileState, RepositorySnapshot, RepositorySource};

pub struct GitRepositorySource;

impl RepositorySource for GitRepositorySource {
    fn snapshot(&self) -> Result<RepositorySnapshot> {
        let output = Command::new("git")
            .args(["diff", "--no-ext-diff", "--color=never"])
            .output()
            .context("failed to run git; is it installed and available on PATH?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git diff failed: {}", stderr.trim());
        }

        let diff = String::from_utf8(output.stdout).context("git returned a non-UTF-8 diff")?;
        let files = if diff.is_empty() {
            Vec::new()
        } else {
            vec![FileState {
                path: PathBuf::from("working tree"),
                old_path: None,
                kind: ChangeKind::Modified,
                staged: None,
                unstaged: Some(FileDiff { text: diff }),
            }]
        };

        Ok(RepositorySnapshot {
            files,
            ..RepositorySnapshot::default()
        })
    }
}
