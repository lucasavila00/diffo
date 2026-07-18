use std::path::{Path, PathBuf};

use diffo_core::{
    FailureKind, OperationFailure, OperationResult, RepositoryAction, RepositorySnapshot,
};

use crate::{CommandId, CommandPalette};

mod commit;
mod navigation;
mod palette;
mod repository;
mod staging;
mod toast;

use navigation::file_keys;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChangeArea {
    Unstaged,
    Staged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileKey {
    pub path: PathBuf,
    pub area: ChangeArea,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FileListScroll {
    pub staged: usize,
    pub unstaged: usize,
}

impl FileListScroll {
    #[must_use]
    pub const fn get(self, area: ChangeArea) -> usize {
        match area {
            ChangeArea::Staged => self.staged,
            ChangeArea::Unstaged => self.unstaged,
        }
    }

    pub fn set(&mut self, area: ChangeArea, position: usize) {
        match area {
            ChangeArea::Staged => self.staged = position,
            ChangeArea::Unstaged => self.unstaged = position,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileContextMenu {
    pub file: FileKey,
    pub column: u16,
    pub row: u16,
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
    pub file_list_scroll: FileListScroll,
    pub file_pane_percent: u16,
    pub resizing_file_pane: bool,
    pub command_palette: Option<CommandPalette>,
    pub help_open: bool,
    pub file_context_menu: Option<FileContextMenu>,
    pub commit_message: String,
    pub toasts: Vec<Toast>,
    commit_composer_state: CommitComposerState,
    commit_message_cursor: usize,
    pending_operation: Option<RepositoryAction>,
    expanded_file_pane_percent: u16,
    cursor: usize,
    next_toast_id: u64,
    selection_after_action: Option<FileKey>,
}

impl Model {
    #[must_use]
    pub fn new(snapshot: RepositorySnapshot) -> Self {
        let selected = file_keys(&snapshot).into_iter().next();
        Self {
            snapshot,
            selected,
            should_quit: false,
            error: None,
            diff_scroll: 0,
            diff_horizontal_scroll: 0,
            diff_view_mode: DiffViewMode::default(),
            file_list_scroll: FileListScroll::default(),
            file_pane_percent: 25,
            resizing_file_pane: false,
            command_palette: None,
            help_open: false,
            file_context_menu: None,
            commit_message: String::new(),
            toasts: Vec::new(),
            commit_composer_state: CommitComposerState::Idle,
            commit_message_cursor: 0,
            pending_operation: None,
            expanded_file_pane_percent: 25,
            cursor: 0,
            next_toast_id: 1,
            selection_after_action: None,
        }
    }
}

#[cfg(test)]
mod model_tests;
