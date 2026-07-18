use std::path::PathBuf;

use diffo_core::AccessMode;

const NO_CHANGE: char = '.';

mod command;
mod operation;
mod snapshot;
mod status;
pub struct GitRepositorySource {
    root: PathBuf,
    access_mode: AccessMode,
}

impl GitRepositorySource {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            access_mode: AccessMode::ReadWrite,
        }
    }

    #[must_use]
    pub fn with_access_mode(mut self, access_mode: AccessMode) -> Self {
        self.access_mode = access_mode;
        self
    }
}

impl Default for GitRepositorySource {
    fn default() -> Self {
        Self::new(".")
    }
}

#[cfg(test)]
mod git_tests;
