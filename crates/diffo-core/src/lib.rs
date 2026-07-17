use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub mod fixture_source;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RepositorySnapshot {
    pub branch: BranchState,
    pub files: Vec<FileState>,
    pub recent_commits: Vec<Commit>,
    pub upstream: Option<UpstreamState>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BranchState {
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileState {
    pub path: PathBuf,
    pub old_path: Option<PathBuf>,
    pub kind: ChangeKind,
    pub staged: Option<FileDiff>,
    pub unstaged: Option<FileDiff>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Conflicted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileDiff {
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Commit {
    pub id: String,
    pub summary: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UpstreamState {
    pub name: String,
    pub ahead: usize,
    pub behind: usize,
}

pub trait RepositorySource {
    /// Build the current repository snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when repository data cannot be read or parsed.
    fn snapshot(&self) -> Result<RepositorySnapshot>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepositoryAction {
    Stage(PathBuf),
    Unstage(PathBuf),
    StageAll,
}

pub trait Repository: RepositorySource {
    /// Change the repository index.
    ///
    /// # Errors
    ///
    /// Returns an error when the action cannot be applied.
    fn apply(&self, action: &RepositoryAction) -> Result<()>;
}
