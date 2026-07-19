#![doc = include_str!("../README.md")]

use std::{
    path::{Path, PathBuf},
    process,
};

const NO_CHANGE: char = '.';

mod askpass;
mod command;
mod explorer;
mod operation;
mod snapshot;
mod status;
pub use askpass::run_askpass_if_requested;
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
