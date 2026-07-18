use std::{fs, path::Path, process::Command};

use anyhow::{Context, Result};
use diffo_core::{ExplorerFile, ExplorerFileContent, Repository};

use super::GitRepositorySource;

impl GitRepositorySource {
    fn explorer_contents(&self, path: &Path) -> Result<(Vec<u8>, bool)> {
        let full_path = self.root.join(path);
        match fs::symlink_metadata(&full_path) {
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
                Ok((bytes, false))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let spec = format!("HEAD:{}", path.to_string_lossy());
                self.git(&["show", &spec])
                    .map(|bytes| (bytes, true))
                    .with_context(|| format!("failed to read deleted file {}", path.display()))
            }
            Err(error) => {
                Err(error).with_context(|| format!("failed to inspect file {}", path.display()))
            }
        }
    }

    fn explorer_patch(&self, path: &Path, deleted: bool) -> Result<String> {
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
            if !bytes.is_empty() || deleted {
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
    fn explorer_paths(&self) -> Result<Vec<std::path::PathBuf>> {
        let output = self.git(&[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])?;
        output
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| {
                std::str::from_utf8(path)
                    .context("git returned a non-UTF-8 Explorer path")
                    .map(std::path::PathBuf::from)
            })
            .collect()
    }

    fn explorer_file(&self, path: &Path) -> Result<ExplorerFile> {
        let (bytes, deleted) = self.explorer_contents(path)?;
        let content = match std::str::from_utf8(&bytes) {
            Ok(text) if !bytes.contains(&0) => ExplorerFileContent::Text(text.to_owned()),
            Ok(_) | Err(_) => ExplorerFileContent::Binary,
        };
        Ok(ExplorerFile {
            content,
            patch: self.explorer_patch(path, deleted)?,
            deleted,
        })
    }

    fn apply(
        &self,
        action: &diffo_core::RepositoryAction,
    ) -> std::result::Result<diffo_core::OperationResult, diffo_core::OperationFailure> {
        self.apply_operation(action)
    }
}
