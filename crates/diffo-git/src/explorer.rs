use std::{
    collections::BTreeSet,
    fs,
    io::Write as _,
    os::unix::ffi::{OsStrExt as _, OsStringExt as _},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use diffo_core::{
    ExplorerFile, ExplorerFileContent, OperationOutcome, Repository, RepositoryOperationContext,
};

use super::GitRepositorySource;

impl GitRepositorySource {
    fn ignored_paths(&self, paths: &[PathBuf]) -> Result<BTreeSet<PathBuf>> {
        if paths.is_empty() {
            return Ok(BTreeSet::new());
        }
        let mut child = Command::new("git")
            .args(["check-ignore", "--stdin", "-z"])
            .current_dir(&self.root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .context("failed to check which Quick Open paths are ignored")?;
        let stdin = child
            .stdin
            .as_mut()
            .context("git check-ignore did not accept path input")?;
        for path in paths {
            stdin
                .write_all(path.as_os_str().as_bytes())
                .context("failed to check which Quick Open paths are ignored")?;
            stdin
                .write_all(&[0])
                .context("failed to check which Quick Open paths are ignored")?;
        }
        let output = child
            .wait_with_output()
            .context("failed to check which Quick Open paths are ignored")?;
        if !output.status.success() && output.status.code() != Some(1) {
            bail!("git failed to check which Quick Open paths are ignored");
        }
        Ok(output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| PathBuf::from(std::ffi::OsString::from_vec(path.to_owned())))
            .collect())
    }

    fn explorer_paths_from(
        directory: &Path,
        worktree: &Path,
        excluded: &BTreeSet<PathBuf>,
        paths: &mut Vec<PathBuf>,
    ) -> Result<()> {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && directory != worktree => {
                return Ok(());
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to read directory {}", directory.display()));
            }
        };
        for entry in entries {
            let entry = entry
                .with_context(|| format!("failed to read directory {}", directory.display()))?;
            let full_path = entry.path();
            if excluded
                .iter()
                .any(|excluded_path| full_path.starts_with(excluded_path))
            {
                continue;
            }
            let metadata = match fs::symlink_metadata(&full_path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to inspect file {}", full_path.display())
                    });
                }
            };
            if metadata.is_dir() {
                Self::explorer_paths_from(&full_path, worktree, excluded, paths)?;
            } else if metadata.is_file() || metadata.file_type().is_symlink() {
                paths.push(
                    full_path
                        .strip_prefix(worktree)
                        .with_context(|| {
                            format!(
                                "Explorer path {} escaped worktree {}",
                                full_path.display(),
                                worktree.display()
                            )
                        })?
                        .to_owned(),
                );
            }
        }
        Ok(())
    }

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
        let ignored = Command::new("git")
            .args(["check-ignore", "--quiet", "--"])
            .arg(path)
            .current_dir(&self.root)
            .output()
            .context("failed to check whether Explorer file is ignored")?;
        if ignored.status.success() {
            return Ok(String::new());
        }
        if ignored.status.code() != Some(1) {
            bail!("git failed to check whether Explorer file is ignored");
        }
        Ok(self.worktree_file_diff(path)?.text)
    }
}

impl Repository for GitRepositorySource {
    fn checkout_history(&self) -> Result<diffo_core::CheckoutHistory> {
        self.checkout_commit_history()
    }

    fn commit_patch(&self, commit_id: &str) -> Result<String> {
        self.recorded_commit_patch(commit_id)
    }

    fn branches(&self) -> Result<Vec<diffo_core::BranchRef>> {
        self.branch_refs()
    }

    fn merge_refs(&self) -> Result<Vec<diffo_core::MergeRef>> {
        self.merge_refs_list()
    }

    fn stashes(&self) -> Result<Vec<diffo_core::StashEntry>> {
        self.stash_entries()
    }

    fn remotes(&self) -> Result<Vec<String>> {
        self.remote_names()
    }

    fn explorer_paths(&self) -> Result<Vec<std::path::PathBuf>> {
        let watch_paths = self.watch_paths()?;
        let mut excluded = watch_paths
            .git_metadata
            .into_iter()
            .filter(|path| path.starts_with(&watch_paths.worktree))
            .collect::<BTreeSet<_>>();
        excluded.insert(watch_paths.worktree.join(".git"));
        let mut paths = Vec::new();
        Self::explorer_paths_from(
            &watch_paths.worktree,
            &watch_paths.worktree,
            &excluded,
            &mut paths,
        )?;
        paths.sort();
        Ok(paths)
    }

    fn quick_open_paths(&self) -> Result<Vec<PathBuf>> {
        let paths = self.explorer_paths()?;
        let ignored = self.ignored_paths(&paths)?;
        Ok(paths
            .into_iter()
            .filter(|path| !ignored.contains(path))
            .collect())
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
