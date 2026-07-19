use std::{path::PathBuf, process::Command};

use anyhow::{Context, Result, bail};

use super::{GitRepositorySource, NotRepository};

impl GitRepositorySource {
    pub(super) fn repository_root(&self) -> Result<PathBuf> {
        let args = ["rev-parse", "--show-toplevel"];
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .env("LC_ALL", "C")
            .output()
            .context("failed to run git; is it installed and available on PATH?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.starts_with("fatal: not a git repository")
                || stderr.starts_with("fatal: this operation must be run in a work tree")
            {
                return Err(NotRepository.into());
            }
            bail!("git {} failed: {}", args.join(" "), stderr.trim());
        }

        let path =
            String::from_utf8(output.stdout).context("git returned a non-UTF-8 repository path")?;
        Ok(PathBuf::from(path.trim()))
    }

    pub(super) fn git(&self, args: &[&str]) -> Result<Vec<u8>> {
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
}
