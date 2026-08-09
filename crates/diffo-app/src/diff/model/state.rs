use std::path::{Path, PathBuf};

use diffo_core::{
    ChangeKind, FailureKind, OperationFailure, OperationResult, RepositoryAction,
    RepositoryOperationState, RepositorySnapshot,
};

mod commit;
mod navigation;
mod repository;
mod staging;
mod toast;

use navigation::file_keys;
pub use toast::ToastQueue;
pub(crate) use toast::{
    operation_failure_error, operation_result_toast, sync_plan_title, sync_progress_label,
};
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ChangeArea {
    Unstaged,
    Staged,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FileKey {
    pub path: PathBuf,
    pub area: ChangeArea,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StageFileAction {
    path: PathBuf,
    next_unstaged: Option<FileKey>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnstageFileAction {
    path: PathBuf,
    next_staged: Option<FileKey>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PendingFileAction {
    StageFile(StageFileAction),
    UnstageFile(UnstageFileAction),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DiffViewMode {
    #[default]
    Inline,
    SideBySide,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkOperation {
    Fetch,
    Sync,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MergePhase {
    Conflicts(usize),
    Ready,
}

impl MergePhase {
    pub(crate) fn from_snapshot(snapshot: &RepositorySnapshot) -> Option<Self> {
        if snapshot.operation != RepositoryOperationState::Merge {
            return None;
        }
        let conflicts = snapshot
            .files
            .iter()
            .filter(|file| file.kind == ChangeKind::Conflicted)
            .count();
        Some(if conflicts == 0 {
            Self::Ready
        } else {
            Self::Conflicts(conflicts)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToastKind {
    Success,
    Info,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub title: String,
    pub detail: Option<String>,
}

impl NetworkOperation {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fetch | Self::Sync => "Fetching",
        }
    }
}

impl DiffViewMode {
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::Inline => Self::SideBySide,
            Self::SideBySide => Self::Inline,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Model {
    pub snapshot: RepositorySnapshot,
    pub selected: Option<FileKey>,
    pub should_quit: bool,
    pub diff_scroll: usize,
    pub diff_horizontal_scroll: usize,
    pub diff_view_mode: DiffViewMode,
    pub file_pane_percent: u16,
    pub resizing_file_pane: bool,
    pub commit_message: String,
    commit_message_cursor: usize,
    pending_operation: Option<RepositoryAction>,
    cursor: usize,
    pending_file_action: Option<PendingFileAction>,
}

impl Model {
    #[must_use]
    pub fn new(snapshot: RepositorySnapshot) -> Self {
        let keys = file_keys(&snapshot);
        let cursor = keys
            .iter()
            .position(|key| key.area == ChangeArea::Unstaged)
            .unwrap_or(0);
        let selected = keys.get(cursor).cloned();
        Self {
            snapshot,
            selected,
            should_quit: false,
            diff_scroll: 0,
            diff_horizontal_scroll: 0,
            diff_view_mode: DiffViewMode::default(),
            file_pane_percent: 25,
            resizing_file_pane: false,
            commit_message: String::new(),
            commit_message_cursor: 0,
            pending_operation: None,
            cursor,
            pending_file_action: None,
        }
    }

    pub(crate) fn merge_phase(&self) -> Option<MergePhase> {
        MergePhase::from_snapshot(&self.snapshot)
    }
}

#[cfg(test)]
mod model_tests;
