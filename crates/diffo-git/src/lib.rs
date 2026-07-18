use std::{
    collections::BTreeSet,
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail};

use diffo_core::{
    AccessMode, BranchState, ChangeKind, Commit, FailureKind, FileDiff, FileState,
    OperationFailure, OperationResult, Repository, RepositoryAction, RepositorySnapshot,
    RepositorySource, UpstreamState,
};

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
