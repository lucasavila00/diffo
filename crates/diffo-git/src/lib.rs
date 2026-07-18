#![doc = include_str!("../README.md")]

use std::path::PathBuf;

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
}

impl GitRepositorySource {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl Default for GitRepositorySource {
    fn default() -> Self {
        Self::new(".")
    }
}

#[cfg(test)]
mod git_tests;
