#![doc = include_str!("../README.md")]

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BranchKind {
    Local,
    Remote,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchRef {
    pub kind: BranchKind,
    pub name: String,
    pub full_ref: String,
    pub object_id: String,
    pub tip_commit_unix_seconds: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckoutTarget {
    pub kind: BranchKind,
    pub full_ref: String,
    pub object_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateBranchTarget {
    pub name: String,
    pub start_point: CreateBranchStartPoint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeleteBranchTarget {
    pub name: String,
    pub full_ref: String,
    pub object_id: String,
    pub force: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CreateBranchStartPoint {
    Head(HeadState),
    Branch(CheckoutTarget),
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
    Sync,
    Commit(String),
    Checkout(Box<CheckoutTarget>),
    CreateBranch(Box<CreateBranchTarget>),
    DeleteBranch(Box<DeleteBranchTarget>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationResult {
    Stage,
    Unstage,
    Fetch { updated_refs: usize },
    Sync { plan: Box<SyncPlan> },
    Commit { hash: String },
    Checkout { branch: String },
    CreateBranch { branch: String },
    DeleteBranch { branch: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncPlan {
    pub branch: String,
    pub upstream: String,
    pub local_only: usize,
    pub upstream_only: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyncProgress {
    Fetching,
    Plan(SyncPlan),
    FastForwarding { branch: String },
    Rebasing { commits: usize },
    Pushing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryUpdate {
    pub generation: u64,
    pub kind: RepositoryUpdateKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepositoryUpdateKind {
    Snapshot(RepositorySnapshot),
    RefreshFailed(String),
    CommandCompleted {
        command_id: ApplicationCommandId,
        action: RepositoryAction,
        result: OperationResult,
        snapshot: RepositorySnapshot,
    },
    CommandFailed {
        command_id: ApplicationCommandId,
        failure: OperationFailure,
    },
    CommandCancelled {
        command_id: ApplicationCommandId,
        action: RepositoryAction,
        snapshot: RepositorySnapshot,
    },
}

#[derive(Clone, Debug, Default)]
pub struct CancellationHandle(Arc<AtomicBool>);

impl CancellationHandle {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationOutcome {
    Completed(OperationResult),
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ApplicationCommandId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RepositoryQueryId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureKind {
    PushRejected,
    Authentication,
    Network,
    RebaseConflict,
    DirtyWorktree,
    NoUpstream,
    UnsupportedHead,
    OperationInProgress,
    MergeCommits,
    RefChanged,
    BranchConflict,
    BranchNotFullyMerged,
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
    ConfirmProtectedBranchPush { destination: String, commits: usize },
}

pub enum PromptAnswer {
    Text(String),
    Confirm,
    Cancel,
}

pub trait PromptHandler: Send + Sync {
    fn prompt(
        &self,
        id: PromptId,
        prompt: GitPrompt,
        cancellation: &CancellationHandle,
    ) -> PromptAnswer;
}

pub trait ProgressHandler: Send + Sync {
    fn progress(&self, progress: SyncProgress);
}

struct IgnoreProgress;

impl ProgressHandler for IgnoreProgress {
    fn progress(&self, _progress: SyncProgress) {}
}

pub struct RepositoryOperationContext {
    pub prompts: Arc<dyn PromptHandler>,
    pub cancellation: CancellationHandle,
    pub progress: Arc<dyn ProgressHandler>,
}

impl RepositoryOperationContext {
    #[must_use]
    pub fn new(prompts: Arc<dyn PromptHandler>, cancellation: CancellationHandle) -> Self {
        Self {
            prompts,
            cancellation,
            progress: Arc::new(IgnoreProgress),
        }
    }

    #[must_use]
    pub fn with_progress(
        prompts: Arc<dyn PromptHandler>,
        cancellation: CancellationHandle,
        progress: Arc<dyn ProgressHandler>,
    ) -> Self {
        Self {
            prompts,
            cancellation,
            progress,
        }
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
    /// List local and remote branches known to the repository.
    ///
    /// # Errors
    ///
    /// Returns an error when branch references cannot be read or parsed.
    fn branches(&self) -> Result<Vec<BranchRef>> {
        anyhow::bail!("branch discovery is unavailable for this repository source")
    }

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
            .filter(|file| file.kind != ChangeKind::Deleted)
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
    /// Returns an operation failure when the action cannot be applied.
    fn apply_with_context(
        &self,
        action: &RepositoryAction,
        context: &RepositoryOperationContext,
    ) -> std::result::Result<OperationOutcome, OperationFailure> {
        if context.cancellation.is_cancelled() {
            return Ok(OperationOutcome::Cancelled);
        }
        self.apply(action).map(OperationOutcome::Completed)
    }
}
