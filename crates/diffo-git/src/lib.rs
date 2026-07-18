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
    askpass: Option<askpass_image::PreparedAskpass>,
}

impl GitRepositorySource {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            askpass: None,
        }
    }

    /// Create a repository source with a private copy of the running executable for
    /// deferred Git and SSH prompts.
    ///
    /// # Errors
    ///
    /// Returns an error when the running executable cannot be copied into a private,
    /// executable runtime location.
    pub fn with_owned_askpass(root: impl Into<PathBuf>) -> anyhow::Result<Self> {
        Ok(Self {
            root: root.into(),
            askpass: Some(askpass_image::PreparedAskpass::prepare()?),
        })
    }

    fn askpass_executable(&self) -> Option<&std::path::Path> {
        self.askpass
            .as_ref()
            .map(askpass_image::PreparedAskpass::executable)
    }
}

impl Default for GitRepositorySource {
    fn default() -> Self {
        Self::new(".")
    }
}

#[cfg(test)]
mod git_tests;
