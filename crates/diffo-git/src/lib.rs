#![doc = include_str!("../README.md")]

use std::path::PathBuf;

const NO_CHANGE: char = '.';

mod askpass;
mod askpass_image;
mod command;
mod explorer;
mod operation;
mod snapshot;
mod status;
pub use askpass::run_askpass_if_requested;
pub struct GitRepositorySource {
    root: PathBuf,
    askpass: Option<askpass_image::OwnedAskpass>,
}

impl GitRepositorySource {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            askpass: None,
        }
    }

    /// Create a repository source that retains the running executable for deferred
    /// Git and SSH prompts.
    ///
    /// # Errors
    ///
    /// Returns an error when the running executable cannot be opened and retained.
    pub fn with_owned_askpass(root: impl Into<PathBuf>) -> anyhow::Result<Self> {
        Ok(Self {
            root: root.into(),
            askpass: Some(askpass_image::OwnedAskpass::capture()?),
        })
    }

    fn askpass_executable(&self) -> anyhow::Result<Option<PathBuf>> {
        self.askpass
            .as_ref()
            .map(askpass_image::OwnedAskpass::executable)
            .transpose()
    }
}

impl Default for GitRepositorySource {
    fn default() -> Self {
        Self::new(".")
    }
}

#[cfg(test)]
mod git_tests;
