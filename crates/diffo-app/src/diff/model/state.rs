use std::path::{Path, PathBuf};

use diffo_core::{
    FailureKind, OperationFailure, OperationResult, RepositoryAction, RepositorySnapshot,
};

mod commit;
mod navigation;
mod repository;
mod staging;
mod toast;

use navigation::file_keys;
pub use toast::ToastQueue;
pub(crate) use toast::{operation_failure_title, operation_result_toast};
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
pub enum PrimaryAction {
    Commit,
    Push,
    Pull,
    PushAndPull,
    Disabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkOperation {
    Fetch,
    Pull,
    Push,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToastKind {
    Success,
    Info,
    Error,
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
            Self::Fetch => "Fetching",
            Self::Pull => "Pulling",
            Self::Push => "Pushing",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum CommitComposerState {
    #[default]
    Idle,
    Focused,
}

impl PrimaryAction {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Commit | Self::Disabled => "Commit",
            Self::Push => "Push",
            Self::Pull => "Pull",
            Self::PushAndPull => "Push + Pull",
        }
    }

    #[must_use]
    pub const fn enabled(self) -> bool {
        matches!(self, Self::Commit | Self::Push | Self::Pull)
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

pub struct Model {
    pub snapshot: RepositorySnapshot,
    pub selected: Option<FileKey>,
    pub should_quit: bool,
    pub error: Option<String>,
    pub diff_scroll: usize,
    pub diff_horizontal_scroll: usize,
    pub diff_view_mode: DiffViewMode,
    pub file_pane_percent: u16,
    pub resizing_file_pane: bool,
    pub help_open: bool,
    pub commit_message: String,
    commit_composer_state: CommitComposerState,
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
            error: None,
            diff_scroll: 0,
            diff_horizontal_scroll: 0,
            diff_view_mode: DiffViewMode::default(),
            file_pane_percent: 25,
            resizing_file_pane: false,
            help_open: false,
            commit_message: String::new(),
            commit_composer_state: CommitComposerState::Idle,
            commit_message_cursor: 0,
            pending_operation: None,
            cursor,
            pending_file_action: None,
        }
    }

    pub fn toggle_help(&mut self) {
        self.help_open = !self.help_open;
    }

    pub fn close_help(&mut self) {
        self.help_open = false;
    }
}

#[cfg(test)]
mod model_tests;
