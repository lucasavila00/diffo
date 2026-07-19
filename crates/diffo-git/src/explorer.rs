use std::{fs, path::Path, process::Command};

use anyhow::{Context, Result, bail};
use diffo_core::{
    ExplorerFile, ExplorerFileContent, OperationOutcome, Repository, RepositoryOperationContext,
};

use super::GitRepositorySource;

impl GitRepositorySource {
    fn explorer_contents(&self, path: &Path) -> Result<Vec<u8>> {
        let full_path = self.root.join(path);
        match fs::symlink_metadata(&full_path) {
            Ok(metadata) if metadata.is_dir() => {
                bail!("Explorer path is a directory: {}", path.display())
            }
            Ok(metadata) => {
                let bytes = if metadata.file_type().is_symlink() {
                    fs::read_link(&full_path)
                        .with_context(|| format!("failed to read symlink {}", path.display()))?
                        .to_string_lossy()
                        .into_owned()
                        .into_bytes()
                } else {
                    fs::read(&full_path)
                        .with_context(|| format!("failed to read file {}", path.display()))?
                };
                Ok(bytes)
            }
            Err(error) => {
                Err(error).with_context(|| format!("failed to inspect file {}", path.display()))
            }
        }
    }

    fn explorer_patch(&self, path: &Path) -> Result<String> {
        let path_text = path.to_string_lossy();
        let has_head = Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&self.root)
            .output()
            .context("failed to check Git HEAD")?
            .status
            .success();
        if has_head {
            let bytes = self.git(&[
                "diff",
                "HEAD",
                "--no-ext-diff",
                "--no-color",
                "--unified=0",
                "--",
                &path_text,
            ])?;
            if !bytes.is_empty() {
                return String::from_utf8(bytes).context("git returned a non-UTF-8 Explorer patch");
            }
            let tracked = Command::new("git")
                .args(["ls-files", "--error-unmatch", "--"])
                .arg(path)
                .current_dir(&self.root)
                .output()
                .context("failed to check whether Explorer file is tracked")?
                .status
                .success();
            if tracked {
                return Ok(String::new());
            }
        }
        Ok(self.worktree_file_diff(path)?.text)
    }
}

impl Repository for GitRepositorySource {
    fn branches(&self) -> Result<Vec<diffo_core::BranchRef>> {
        self.branch_refs()
    }

    fn explorer_paths(&self) -> Result<Vec<std::path::PathBuf>> {
        let output = self.git(&[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])?;
        let paths = output
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| {
                std::str::from_utf8(path)
                    .context("git returned a non-UTF-8 Explorer path")
                    .map(std::path::PathBuf::from)
            })
            .collect::<Result<Vec<_>>>()?;
        paths
            .into_iter()
            .filter_map(|path| match fs::symlink_metadata(self.root.join(&path)) {
                Ok(metadata) if !metadata.is_dir() => Some(Ok(path)),
                Ok(_) => None,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => Some(
                    Err(error)
                        .with_context(|| format!("failed to inspect file {}", path.display())),
                ),
            })
            .collect()
    }

    fn explorer_file(&self, path: &Path) -> Result<ExplorerFile> {
        let bytes = self.explorer_contents(path)?;
        let content = match std::str::from_utf8(&bytes) {
            Ok(text) if !bytes.contains(&0) => ExplorerFileContent::Text(text.to_owned()),
            Ok(_) | Err(_) => ExplorerFileContent::Binary,
        };
        Ok(ExplorerFile {
            content,
            patch: self.explorer_patch(path)?,
        })
    }

    fn apply(
        &self,
        action: &diffo_core::RepositoryAction,
    ) -> std::result::Result<diffo_core::OperationResult, diffo_core::OperationFailure> {
        match self.apply_operation(action, None)? {
            OperationOutcome::Completed(result) => Ok(result),
            OperationOutcome::Cancelled => unreachable!("an uncancellable operation was cancelled"),
        }
    }

    fn apply_with_context(
        &self,
        action: &diffo_core::RepositoryAction,
        context: &RepositoryOperationContext,
    ) -> std::result::Result<OperationOutcome, diffo_core::OperationFailure> {
        self.apply_operation(action, Some(context))
    }
}
