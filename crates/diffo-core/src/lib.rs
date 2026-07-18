#![doc = include_str!("../README.md")]

use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub mod fixture_source;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RepositorySnapshot {
    pub head: HeadState,
    pub files: Vec<FileState>,
    pub recent_commits: Vec<Commit>,
    pub upstream: Option<UpstreamState>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum HeadState {
    Named { name: String, commit: String },
    Unborn { name: String },
    Detached { commit: String },
}

impl Default for HeadState {
    fn default() -> Self {
        Self::Unborn {
            name: "HEAD".to_owned(),
        }
    }
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExplorerFile {
    pub content: ExplorerFileContent,
    pub patch: String,
    pub deleted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExplorerFileContent {
    Text(String),
    Binary,
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

pub trait RepositorySource: Send + Sync {
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
    UnstageAll,
    Fetch,
    Pull,
    Push,
    Commit(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationResult {
    Stage,
    Unstage,
    Fetch { updated_refs: usize },
    Pull { commits: usize },
    Push { hash: String, upstream: String },
    Commit { hash: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureKind {
    Cancelled,
    PullRequired,
    Diverged,
    PushRejected,
    Authentication,
    Network,
    MergeConflict,
    DirtyWorktree,
    HookRejected,
    NoRemote,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PromptId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretKind {
    HttpsSecret,
    SshKeyPassphrase,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitPrompt {
    Username { host: String },
    Secret { kind: SecretKind, context: String },
    ConfirmSshHost { host: String, fingerprint: String },
}

pub enum PromptAnswer {
    Text(String),
    Confirm,
    Cancel,
}

pub trait PromptHandler: Send + Sync {
    fn prompt(&self, id: PromptId, prompt: GitPrompt, cancelled: &AtomicBool) -> PromptAnswer;
}

pub struct RepositoryOperationContext {
    pub prompts: Arc<dyn PromptHandler>,
    pub cancelled: Arc<AtomicBool>,
}

impl RepositoryOperationContext {
    #[must_use]
    pub fn new(prompts: Arc<dyn PromptHandler>, cancelled: Arc<AtomicBool>) -> Self {
        Self { prompts, cancelled }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationFailure {
    pub action: RepositoryAction,
    pub kind: FailureKind,
    pub detail: String,
}

impl std::fmt::Display for OperationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.detail)
    }
}

impl std::error::Error for OperationFailure {}

pub trait Repository: RepositorySource {
    /// List tracked and non-ignored untracked repository paths.
    ///
    /// # Errors
    ///
    /// Returns an error when repository paths cannot be read.
    fn explorer_paths(&self) -> Result<Vec<PathBuf>> {
        Ok(self
            .snapshot()?
            .files
            .into_iter()
            .map(|file| file.path)
            .collect())
    }

    /// Read one file for Explorer without adding it to the shared snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when the selected file cannot be read.
    fn explorer_file(&self, _path: &std::path::Path) -> Result<ExplorerFile> {
        anyhow::bail!("file viewing is unavailable for this repository source")
    }

    /// Change the repository index.
    ///
    /// # Errors
    ///
    /// Returns an error when the action cannot be applied.
    fn apply(
        &self,
        action: &RepositoryAction,
    ) -> std::result::Result<OperationResult, OperationFailure>;

    /// Apply an action with access to operation-scoped prompts and cancellation.
    ///
    /// Repository implementations that do not interact with users can use the default
    /// implementation, which delegates to [`Repository::apply`].
    ///
    /// # Errors
    ///
    /// Returns an operation failure when the action cannot be applied or is cancelled.
    fn apply_with_context(
        &self,
        action: &RepositoryAction,
        _context: &RepositoryOperationContext,
    ) -> std::result::Result<OperationResult, OperationFailure> {
        self.apply(action)
    }
}
