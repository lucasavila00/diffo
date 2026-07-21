#![doc = include_str!("../README.md")]

use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
    process,
};

use anyhow::Result;

const NO_CHANGE: char = '.';

mod askpass;
mod branches;
mod command;
mod create_branch;
mod delete_branch;
mod everyday;
mod explorer;
mod failure;
mod operation;
#[cfg(test)]
mod operation_tests;
mod refs;
mod snapshot;
mod stashes;
mod status;
pub use askpass::run_askpass_if_requested;

#[derive(Debug)]
pub struct NotRepository;

impl fmt::Display for NotRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the path is not inside a Git repository")
    }
}

impl Error for NotRepository {}

pub struct GitRepositorySource {
    root: PathBuf,
    askpass: Option<PathBuf>,
}

impl GitRepositorySource {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            askpass: None,
        }
    }

    /// Create a repository source that re-enters the running Diffo process image for
    /// deferred Git and SSH prompts.
    #[must_use]
    pub fn with_askpass(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            askpass: Some(PathBuf::from(format!("/proc/{}/exe", process::id()))),
        }
    }

    /// Discover the enclosing worktree and create a repository source rooted at its
    /// top-level directory with deferred Git and SSH prompt support.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot resolve a repository worktree from `root`.
    pub fn discover_with_askpass(root: impl Into<PathBuf>) -> Result<Self> {
        let mut source = Self::with_askpass(root);
        source.root = source.repository_root()?;
        Ok(source)
    }

    fn askpass_executable(&self) -> Option<&Path> {
        self.askpass.as_deref()
    }
}

impl Default for GitRepositorySource {
    fn default() -> Self {
        Self::new(".")
    }
}

#[cfg(test)]
mod git_tests;
