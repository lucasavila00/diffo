use std::path::{Path, PathBuf};

use diffo_core::{
    AccessMode, FailureKind, OperationFailure, OperationResult, RepositoryAction,
    RepositorySnapshot,
};

use crate::{CommandId, CommandPalette};

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
    pub access_mode: AccessMode,
    pub diff_scroll: usize,
    pub diff_horizontal_scroll: usize,
    pub diff_view_mode: DiffViewMode,
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
    pub fn new(snapshot: RepositorySnapshot, access_mode: AccessMode) -> Self {
        let selected = file_keys(&snapshot).into_iter().next();
        Self {
            snapshot,
            selected,
            should_quit: false,
            error: None,
            access_mode,
            diff_scroll: 0,
            diff_horizontal_scroll: 0,
            diff_view_mode: DiffViewMode::default(),
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

    pub fn open_command_palette(&mut self) {
        self.help_open = false;
        self.command_palette = Some(CommandPalette::default());
    }

    pub fn close_command_palette(&mut self) {
        self.command_palette = None;
    }

    pub fn toggle_help(&mut self) {
        self.command_palette = None;
        self.help_open = !self.help_open;
    }

    pub fn close_help(&mut self) {
        self.help_open = false;
    }

    pub fn focus_commit_input(&mut self) {
        if self.access_mode == AccessMode::ReadWrite {
            self.command_palette = None;
            self.help_open = false;
            self.commit_composer_state = CommitComposerState::Focused;
        }
    }

    pub fn blur_commit_input(&mut self) {
        self.commit_composer_state = CommitComposerState::Idle;
    }

    #[must_use]
    pub fn commit_input_focused(&self) -> bool {
        self.commit_composer_state == CommitComposerState::Focused
    }

    pub fn commit_message_input(&mut self, character: char) {
        if self.commit_input_focused() && !character.is_control() {
            let byte = byte_index_at_char(&self.commit_message, self.commit_message_cursor);
            self.commit_message.insert(byte, character);
            self.commit_message_cursor = self.commit_message_cursor.saturating_add(1);
        }
    }

    pub fn commit_message_backspace(&mut self) {
        if self.commit_input_focused() && self.commit_message_cursor > 0 {
            let start = byte_index_at_char(
                &self.commit_message,
                self.commit_message_cursor.saturating_sub(1),
            );
            let end = byte_index_at_char(&self.commit_message, self.commit_message_cursor);
            self.commit_message.replace_range(start..end, "");
            self.commit_message_cursor = self.commit_message_cursor.saturating_sub(1);
        }
    }

    pub fn commit_message_cursor_left(&mut self) {
        if self.commit_input_focused() {
            self.commit_message_cursor = self.commit_message_cursor.saturating_sub(1);
        }
    }

    pub fn commit_message_cursor_right(&mut self) {
        if self.commit_input_focused() {
            self.commit_message_cursor = self
                .commit_message_cursor
                .saturating_add(1)
                .min(self.commit_message.chars().count());
        }
    }

    #[must_use]
    pub fn commit_message_cursor(&self) -> usize {
        self.commit_message_cursor
    }

    #[must_use]
    pub fn suggested_commit_message(&self) -> Option<String> {
        let staged_files = self
            .snapshot
            .files
            .iter()
            .filter(|file| file.staged.is_some())
            .count();
        match staged_files {
            0 => None,
            1 => Some("Update 1 file".to_owned()),
            count => Some(format!("Update {count} files")),
        }
    }

    fn effective_commit_message(&self) -> Option<String> {
        let message = self.commit_message.trim();
        if message.is_empty() {
            self.suggested_commit_message()
        } else {
            Some(message.to_owned())
        }
    }

    #[must_use]
    pub fn primary_action(&self) -> PrimaryAction {
        if self.access_mode == AccessMode::ReadOnly {
            return PrimaryAction::Disabled;
        }
        if let Some(action) = self.pending_operation.as_ref() {
            return match action {
                RepositoryAction::Commit(_) => PrimaryAction::Commit,
                RepositoryAction::Push => PrimaryAction::Push,
                RepositoryAction::Pull => PrimaryAction::Pull,
                _ => PrimaryAction::Disabled,
            };
        }
        if self.effective_commit_message().is_some() {
            return PrimaryAction::Commit;
        }
        match self.snapshot.upstream.as_ref() {
            Some(upstream) if upstream.ahead > 0 && upstream.behind > 0 => {
                PrimaryAction::PushAndPull
            }
            Some(upstream) if upstream.behind > 0 => PrimaryAction::Pull,
            Some(upstream) if upstream.ahead > 0 => PrimaryAction::Push,
            _ => PrimaryAction::Disabled,
        }
    }

    #[must_use]
    pub fn primary_action_enabled(&self) -> bool {
        self.pending_operation.is_none() && self.primary_action().enabled()
    }

    pub fn execute_primary_action(&mut self) -> Option<RepositoryAction> {
        let primary = self.primary_action();
        if primary == PrimaryAction::PushAndPull {
            self.show_operation_failure(&OperationFailure {
                action: RepositoryAction::Push,
                kind: FailureKind::PullRequired,
                detail: "pull and merge required".to_owned(),
            });
            return None;
        }
        if !self.primary_action_enabled() {
            return None;
        }
        let action = match primary {
            PrimaryAction::Commit => RepositoryAction::Commit(self.effective_commit_message()?),
            PrimaryAction::Push => RepositoryAction::Push,
            PrimaryAction::Pull => RepositoryAction::Pull,
            PrimaryAction::PushAndPull | PrimaryAction::Disabled => return None,
        };
        self.commit_composer_state = CommitComposerState::Idle;
        self.error = None;
        self.pending_operation = Some(action.clone());
        Some(action)
    }

    #[must_use]
    pub fn network_operation(&self) -> Option<NetworkOperation> {
        match self.pending_operation.as_ref() {
            Some(RepositoryAction::Fetch) => Some(NetworkOperation::Fetch),
            Some(RepositoryAction::Pull) => Some(NetworkOperation::Pull),
            Some(RepositoryAction::Push) => Some(NetworkOperation::Push),
            _ => None,
        }
    }

    pub fn command_palette_input(&mut self, character: char) {
        if let Some(palette) = self.command_palette.as_mut() {
            palette.push(character);
        }
    }

    pub fn command_palette_backspace(&mut self) {
        if let Some(palette) = self.command_palette.as_mut() {
            palette.backspace();
        }
    }

    pub fn command_palette_select_previous(&mut self) {
        if let Some(palette) = self.command_palette.as_mut() {
            palette.select_previous();
        }
    }

    pub fn command_palette_select_next(&mut self) {
        if let Some(palette) = self.command_palette.as_mut() {
            palette.select_next();
        }
    }

    pub fn command_palette_select(&mut self, index: usize) {
        if let Some(palette) = self.command_palette.as_mut() {
            palette.select(index);
        }
    }

    pub fn execute_selected_command(&mut self) -> Option<RepositoryAction> {
        if self.access_mode == AccessMode::ReadOnly || self.pending_operation.is_some() {
            return None;
        }
        let command = self.command_palette.as_ref()?.selected_command()?.id;
        self.command_palette = None;
        let action = match command {
            CommandId::Fetch => RepositoryAction::Fetch,
            CommandId::Pull => RepositoryAction::Pull,
        };
        self.error = None;
        self.pending_operation = Some(action.clone());
        Some(action)
    }

    pub fn select_next(&mut self) {
        let keys = file_keys(&self.snapshot);
        if keys.is_empty() {
            return;
        }
        self.cursor = self.cursor.saturating_add(1).min(keys.len() - 1);
        self.selected = keys.get(self.cursor).cloned();
        self.error = None;
    }

    pub fn select_previous(&mut self) {
        let keys = file_keys(&self.snapshot);
        self.cursor = self.cursor.saturating_sub(1);
        self.selected = keys.get(self.cursor).cloned();
        self.error = None;
    }

    pub fn select_first(&mut self) {
        self.cursor = 0;
        self.selected = file_keys(&self.snapshot).into_iter().next();
    }

    pub fn select_last(&mut self) {
        let keys = file_keys(&self.snapshot);
        self.cursor = keys.len().saturating_sub(1);
        self.selected = keys.get(self.cursor).cloned();
    }

    pub fn select_file(&mut self, file: &FileKey) {
        let keys = file_keys(&self.snapshot);
        if let Some(cursor) = keys.iter().position(|key| key == file) {
            self.cursor = cursor;
            self.selected = keys.get(cursor).cloned();
            self.error = None;
        }
    }

    pub fn open_file_context_menu(&mut self, file: FileKey, column: u16, row: u16) {
        self.select_file(&file);
        self.file_context_menu = Some(FileContextMenu { file, column, row });
    }

    pub fn close_file_context_menu(&mut self) {
        self.file_context_menu = None;
    }

    pub fn copy_context_path(&mut self, absolute: bool) -> Option<crate::Effect> {
        let path = self.file_context_menu.take()?.file.path;
        Some(crate::Effect::CopyPath { path, absolute })
    }

    pub fn scroll_diff_down(&mut self) {
        self.scroll_diff_down_by(4);
    }

    pub fn scroll_diff_up(&mut self) {
        self.scroll_diff_up_by(4);
    }

    pub fn scroll_diff_down_by(&mut self, lines: usize) {
        self.diff_scroll = self.diff_scroll.saturating_add(lines);
    }

    pub fn scroll_diff_up_by(&mut self, lines: usize) {
        self.diff_scroll = self.diff_scroll.saturating_sub(lines);
    }

    pub fn scroll_diff_by(&mut self, lines: i64) {
        let magnitude = usize::try_from(lines.unsigned_abs()).unwrap_or(usize::MAX);
        if lines >= 0 {
            self.diff_scroll = self.diff_scroll.saturating_add(magnitude);
        } else {
            self.diff_scroll = self.diff_scroll.saturating_sub(magnitude);
        }
    }

    pub fn scroll_diff_right(&mut self) {
        self.diff_horizontal_scroll = self.diff_horizontal_scroll.saturating_add(4);
    }

    pub fn scroll_diff_left(&mut self) {
        self.diff_horizontal_scroll = self.diff_horizontal_scroll.saturating_sub(4);
    }

    pub fn scroll_diff_horizontal_by(&mut self, columns: i64) {
        let magnitude = usize::try_from(columns.unsigned_abs()).unwrap_or(usize::MAX);
        if columns >= 0 {
            self.diff_horizontal_scroll = self.diff_horizontal_scroll.saturating_add(magnitude);
        } else {
            self.diff_horizontal_scroll = self.diff_horizontal_scroll.saturating_sub(magnitude);
        }
    }

    pub fn clamp_diff_scroll(&mut self, maximum_row: usize, maximum_column: usize) {
        self.diff_scroll = self.diff_scroll.min(maximum_row);
        self.diff_horizontal_scroll = self.diff_horizontal_scroll.min(maximum_column);
    }

    pub fn set_diff_viewport(&mut self, vertical: usize, horizontal: usize) {
        self.diff_scroll = vertical;
        self.diff_horizontal_scroll = horizontal;
    }

    pub fn toggle_diff_view(&mut self) {
        self.diff_view_mode = self.diff_view_mode.toggled();
        self.reset_diff_scroll();
    }

    pub fn toggle_file_pane(&mut self) {
        if self.file_pane_percent == 0 {
            self.file_pane_percent = self.expanded_file_pane_percent;
        } else {
            self.expanded_file_pane_percent = self.file_pane_percent;
            self.file_pane_percent = 0;
        }
        self.resizing_file_pane = false;
    }

    pub fn begin_file_pane_resize(&mut self) {
        self.resizing_file_pane = true;
    }

    pub fn resize_file_pane(&mut self, percent: u16) {
        if self.resizing_file_pane {
            self.file_pane_percent = percent.min(80);
            if self.file_pane_percent > 0 {
                self.expanded_file_pane_percent = self.file_pane_percent;
            }
        }
    }

    pub fn end_file_pane_resize(&mut self) {
        self.resizing_file_pane = false;
    }

    #[must_use]
    pub fn stage_selected(&self) -> Option<RepositoryAction> {
        if self.access_mode == AccessMode::ReadOnly {
            return None;
        }
        self.selected.as_ref().and_then(|key| {
            (key.area == ChangeArea::Unstaged).then(|| RepositoryAction::Stage(key.path.clone()))
        })
    }

    #[must_use]
    pub fn toggle_stage_selected(&mut self) -> Option<RepositoryAction> {
        let selected = self.selected.clone()?;
        let action = match selected.area {
            ChangeArea::Unstaged => self.stage_selected(),
            ChangeArea::Staged => self.unstage_selected(),
        };
        if action.is_some() {
            let peers = file_keys(&self.snapshot)
                .into_iter()
                .filter(|key| key.area == selected.area)
                .collect::<Vec<_>>();
            self.selection_after_action =
                peers
                    .iter()
                    .position(|key| key == &selected)
                    .and_then(|index| {
                        (peers.len() > 1).then(|| peers[(index + 1) % peers.len()].clone())
                    });
        }
        action
    }

    #[must_use]
    pub fn unstage_selected(&self) -> Option<RepositoryAction> {
        if self.access_mode == AccessMode::ReadOnly {
            return None;
        }
        self.selected.as_ref().and_then(|key| {
            (key.area == ChangeArea::Staged).then(|| RepositoryAction::Unstage(key.path.clone()))
        })
    }

    #[must_use]
    pub fn toggle_stage_all(&self) -> Option<RepositoryAction> {
        if self.access_mode == AccessMode::ReadOnly {
            return None;
        }
        if self
            .snapshot
            .files
            .iter()
            .any(|file| file.unstaged.is_some() || file.kind == diffo_core::ChangeKind::Untracked)
        {
            Some(RepositoryAction::StageAll)
        } else {
            self.snapshot
                .files
                .iter()
                .any(|file| file.staged.is_some())
                .then_some(RepositoryAction::UnstageAll)
        }
    }

    #[must_use]
    pub fn stage_all(&self) -> Option<RepositoryAction> {
        (self.access_mode == AccessMode::ReadWrite
            && self.snapshot.files.iter().any(|file| {
                file.unstaged.is_some() || file.kind == diffo_core::ChangeKind::Untracked
            }))
        .then_some(RepositoryAction::StageAll)
    }

    #[must_use]
    pub fn unstage_all(&self) -> Option<RepositoryAction> {
        (self.access_mode == AccessMode::ReadWrite
            && self.snapshot.files.iter().any(|file| file.staged.is_some()))
        .then_some(RepositoryAction::UnstageAll)
    }

    #[must_use]
    pub fn stage_file(&self, path: PathBuf) -> Option<RepositoryAction> {
        (self.access_mode == AccessMode::ReadWrite
            && self.snapshot.files.iter().any(|file| {
                file.path == path
                    && (file.unstaged.is_some() || file.kind == diffo_core::ChangeKind::Untracked)
            }))
        .then_some(RepositoryAction::Stage(path))
    }

    #[must_use]
    pub fn unstage_file(&self, path: PathBuf) -> Option<RepositoryAction> {
        (self.access_mode == AccessMode::ReadWrite
            && self
                .snapshot
                .files
                .iter()
                .any(|file| file.path == path && file.staged.is_some()))
        .then_some(RepositoryAction::Unstage(path))
    }

    pub fn repository_changed(&mut self, snapshot: RepositorySnapshot) {
        self.install_snapshot(snapshot, false);
    }

    fn install_snapshot(&mut self, snapshot: RepositorySnapshot, action_completed: bool) {
        let intended_selection = action_completed
            .then(|| self.selection_after_action.take())
            .flatten();
        let old_selected = self.selected.clone();
        let old_cursor = self.cursor;
        let keys = file_keys(&snapshot);

        let cursor = intended_selection
            .as_ref()
            .and_then(|selected| keys.iter().position(|key| key == selected))
            .or_else(|| {
                old_selected
                    .as_ref()
                    .and_then(|selected| keys.iter().position(|key| key == selected))
            })
            .unwrap_or_else(|| old_cursor.min(keys.len().saturating_sub(1)));
        let selected = keys.get(cursor).cloned();
        self.snapshot = snapshot;
        self.cursor = cursor;
        self.selected = selected;
        self.error = None;
    }

    fn finish_pending_operation(&mut self) {
        if matches!(self.pending_operation, Some(RepositoryAction::Commit(_))) {
            self.commit_message.clear();
            self.commit_message_cursor = 0;
        }
        self.pending_operation = None;
    }

    pub fn show_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    pub fn complete_operation(&mut self, result: &OperationResult, snapshot: RepositorySnapshot) {
        let is_async_result = matches!(
            result,
            OperationResult::Fetch { .. }
                | OperationResult::Pull { .. }
                | OperationResult::Push { .. }
                | OperationResult::Commit { .. }
        );
        let finishes_pending = matches!(
            (self.pending_operation.as_ref(), result),
            (Some(RepositoryAction::Fetch), OperationResult::Fetch { .. })
                | (Some(RepositoryAction::Pull), OperationResult::Pull { .. })
                | (Some(RepositoryAction::Push), OperationResult::Push { .. })
                | (
                    Some(RepositoryAction::Commit(_)),
                    OperationResult::Commit { .. }
                )
        );
        if is_async_result && !finishes_pending {
            self.install_snapshot(snapshot, false);
            return;
        }
        if finishes_pending {
            self.finish_pending_operation();
        }
        self.install_snapshot(snapshot, true);
        if let Some((kind, title)) = operation_result_toast(result) {
            self.push_toast(kind, title, None);
        }
    }

    pub fn show_operation_failure(&mut self, failure: &OperationFailure) {
        if let Some(pending) = self.pending_operation.as_ref()
            && !same_repository_operation(pending, &failure.action)
        {
            return;
        }
        if matches!(self.pending_operation, Some(RepositoryAction::Commit(_))) {
            self.commit_composer_state = CommitComposerState::Focused;
        }
        self.pending_operation = None;
        self.selection_after_action = None;
        self.error = None;
        self.push_toast(ToastKind::Error, operation_failure_title(failure), None);
    }

    pub fn dismiss_toast(&mut self, id: u64) {
        self.toasts.retain(|toast| toast.id != id);
    }

    pub fn show_toast(&mut self, kind: ToastKind, title: impl Into<String>) {
        self.push_toast(kind, title.into(), None);
    }

    fn push_toast(&mut self, kind: ToastKind, title: String, detail: Option<String>) {
        self.toasts
            .retain(|toast| toast.title != title || toast.detail != detail);
        let toast = Toast {
            id: self.next_toast_id,
            kind,
            title,
            detail,
        };
        self.next_toast_id = self.next_toast_id.saturating_add(1);
        self.toasts.insert(0, toast);
        self.toasts.truncate(3);
    }

    #[must_use]
    pub fn is_selected(&self, path: &Path, area: ChangeArea) -> bool {
        self.selected
            .as_ref()
            .is_some_and(|key| key.path == path && key.area == area)
    }

    fn reset_diff_scroll(&mut self) {
        self.diff_scroll = 0;
        self.diff_horizontal_scroll = 0;
    }
}

fn same_repository_operation(left: &RepositoryAction, right: &RepositoryAction) -> bool {
    matches!(
        (left, right),
        (RepositoryAction::Fetch, RepositoryAction::Fetch)
            | (RepositoryAction::Pull, RepositoryAction::Pull)
            | (RepositoryAction::Push, RepositoryAction::Push)
            | (RepositoryAction::Commit(_), RepositoryAction::Commit(_))
            | (RepositoryAction::Stage(_), RepositoryAction::Stage(_))
            | (RepositoryAction::Unstage(_), RepositoryAction::Unstage(_))
            | (RepositoryAction::StageAll, RepositoryAction::StageAll)
            | (RepositoryAction::UnstageAll, RepositoryAction::UnstageAll)
    )
}

fn byte_index_at_char(text: &str, character: usize) -> usize {
    text.char_indices()
        .nth(character)
        .map_or(text.len(), |(index, _)| index)
}

fn operation_result_toast(result: &OperationResult) -> Option<(ToastKind, String)> {
    let title = match result {
        OperationResult::Stage | OperationResult::Unstage => return None,
        OperationResult::Fetch { updated_refs: 0 } => "Fetch complete".to_owned(),
        OperationResult::Fetch { updated_refs: 1 } => "Fetched 1 ref".to_owned(),
        OperationResult::Fetch { updated_refs } => format!("Fetched {updated_refs} refs"),
        OperationResult::Pull { commits: 0 } => "Already up to date".to_owned(),
        OperationResult::Pull { commits: 1 } => "Pulled 1 commit".to_owned(),
        OperationResult::Pull { commits } => format!("Pulled {commits} commits"),
        OperationResult::Push { hash, upstream } => {
            format!("Pushed {} to {upstream}", short_hash(hash))
        }
        OperationResult::Commit { hash } => format!("Committed {}", short_hash(hash)),
    };
    Some((ToastKind::Success, title))
}

fn operation_failure_title(failure: &OperationFailure) -> String {
    let action = match &failure.action {
        RepositoryAction::Stage(_) | RepositoryAction::StageAll => "Stage",
        RepositoryAction::Unstage(_) | RepositoryAction::UnstageAll => "Unstage",
        RepositoryAction::Fetch => "Fetch",
        RepositoryAction::Pull => "Pull",
        RepositoryAction::Push => "Push",
        RepositoryAction::Commit(_) => "Commit",
    };
    match failure.kind {
        FailureKind::PullRequired => format!("Push blocked: {}", failure.detail),
        FailureKind::Diverged => format!("Pull blocked: {}", failure.detail),
        FailureKind::PushRejected | FailureKind::HookRejected => {
            format!("Push rejected: {}", failure.detail)
        }
        FailureKind::MergeConflict => format!("Pull stopped: {}", failure.detail),
        _ => format!("{action} failed: {}", failure.detail),
    }
}

fn short_hash(hash: &str) -> &str {
    hash.get(..7.min(hash.len())).unwrap_or(hash)
}

fn file_keys(snapshot: &RepositorySnapshot) -> Vec<FileKey> {
    staged_files(snapshot)
        .map(|file| FileKey {
            path: file.path.clone(),
            area: ChangeArea::Staged,
        })
        .chain(unstaged_files(snapshot).map(|file| FileKey {
            path: file.path.clone(),
            area: ChangeArea::Unstaged,
        }))
        .collect()
}

pub(crate) fn unstaged_files(
    snapshot: &RepositorySnapshot,
) -> impl Iterator<Item = &diffo_core::FileState> {
    snapshot
        .files
        .iter()
        .filter(|file| file.unstaged.is_some() || file.kind == diffo_core::ChangeKind::Untracked)
}

pub(crate) fn staged_files(
    snapshot: &RepositorySnapshot,
) -> impl Iterator<Item = &diffo_core::FileState> {
    snapshot.files.iter().filter(|file| file.staged.is_some())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use diffo_core::{
        AccessMode, ChangeKind, FileDiff, FileState, OperationResult, RepositoryAction,
        RepositorySnapshot,
    };

    use super::{ChangeArea, FileKey, Model};

    fn snapshot() -> RepositorySnapshot {
        RepositorySnapshot {
            files: vec![
                FileState {
                    path: PathBuf::from("both.txt"),
                    old_path: None,
                    kind: ChangeKind::Modified,
                    staged: Some(FileDiff {
                        text: String::new(),
                    }),
                    unstaged: Some(FileDiff {
                        text: String::new(),
                    }),
                },
                FileState {
                    path: PathBuf::from("new.txt"),
                    old_path: None,
                    kind: ChangeKind::Untracked,
                    staged: None,
                    unstaged: None,
                },
            ],
            ..RepositorySnapshot::default()
        }
    }

    #[test]
    fn navigates_both_groups() {
        let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
        assert_eq!(
            app.selected.as_ref().expect("selection").path,
            PathBuf::from("both.txt")
        );

        assert_eq!(
            app.selected.as_ref().expect("selection").area,
            ChangeArea::Staged
        );
        app.select_next();
        assert_eq!(
            app.selected.as_ref().expect("selection").path,
            PathBuf::from("both.txt")
        );
        assert_eq!(
            app.selected.as_ref().expect("selection").area,
            ChangeArea::Unstaged
        );
        app.select_next();
        assert_eq!(
            app.selected.as_ref().expect("selection").path,
            PathBuf::from("new.txt")
        );
    }

    #[test]
    fn creates_actions_for_the_selected_group() {
        let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
        assert_eq!(app.stage_selected(), None);
        assert_eq!(
            app.unstage_selected(),
            Some(RepositoryAction::Unstage(PathBuf::from("both.txt")))
        );

        app.select_next();
        assert_eq!(app.unstage_selected(), None);
        assert_eq!(
            app.stage_selected(),
            Some(RepositoryAction::Stage(PathBuf::from("both.txt")))
        );
    }

    #[test]
    fn staging_for_review_selects_the_next_unstaged_file_after_refresh() {
        let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
        app.select_next();
        assert_eq!(
            app.selected,
            Some(FileKey {
                path: PathBuf::from("both.txt"),
                area: ChangeArea::Unstaged,
            })
        );

        assert_eq!(
            app.toggle_stage_selected(),
            Some(RepositoryAction::Stage(PathBuf::from("both.txt")))
        );
        let mut refreshed = snapshot();
        refreshed.files[0].unstaged = None;
        app.complete_operation(&OperationResult::Stage, refreshed);

        assert_eq!(
            app.selected,
            Some(FileKey {
                path: PathBuf::from("new.txt"),
                area: ChangeArea::Unstaged,
            })
        );
    }

    #[test]
    fn keeps_selection_after_refresh() {
        let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
        let selected = FileKey {
            path: PathBuf::from("both.txt"),
            area: ChangeArea::Staged,
        };

        app.repository_changed(snapshot());

        assert_eq!(app.selected, Some(selected));
    }

    #[test]
    fn preserves_scroll_when_the_selected_file_changes_content() {
        let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
        app.diff_scroll = 12;
        app.diff_horizontal_scroll = 8;

        app.repository_changed(snapshot());
        assert_eq!(app.diff_scroll, 12);
        assert_eq!(app.diff_horizontal_scroll, 8);

        let mut changed = snapshot();
        changed.files[0]
            .staged
            .as_mut()
            .expect("staged diff")
            .text
            .push_str("changed");
        app.repository_changed(changed);
        assert_eq!(app.diff_scroll, 12);
        assert_eq!(app.diff_horizontal_scroll, 8);
    }

    #[test]
    fn preserves_commit_input_focus_across_repository_refresh() {
        let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
        app.focus_commit_input();

        app.repository_changed(snapshot());
        app.commit_message_input('x');

        assert!(app.commit_input_focused());
        assert_eq!(app.commit_message, "x");
    }

    #[test]
    fn edits_commit_message_at_a_preserved_character_cursor() {
        let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
        app.focus_commit_input();
        for character in "ac".chars() {
            app.commit_message_input(character);
        }
        app.commit_message_cursor_left();
        app.commit_message_input('b');
        app.blur_commit_input();
        app.focus_commit_input();

        assert_eq!(app.commit_message, "abc");
        assert_eq!(app.commit_message_cursor(), 2);
        app.commit_message_backspace();
        assert_eq!(app.commit_message, "ac");
    }

    #[test]
    fn read_only_mode_blocks_actions() {
        let mut app = Model::new(snapshot(), AccessMode::ReadOnly);

        assert_eq!(app.stage_selected(), None);
        assert_eq!(app.toggle_stage_all(), None);
        app.focus_commit_input();
        assert!(!app.commit_input_focused());
    }

    #[test]
    fn queues_replaces_limits_and_dismisses_toasts() {
        let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
        for updated_refs in 1..=4 {
            app.open_command_palette();
            assert_eq!(
                app.execute_selected_command(),
                Some(RepositoryAction::Fetch)
            );
            app.complete_operation(&OperationResult::Fetch { updated_refs }, snapshot());
        }
        assert_eq!(app.toasts.len(), 3);
        assert_eq!(app.toasts[0].title, "Fetched 4 refs");

        app.open_command_palette();
        assert_eq!(
            app.execute_selected_command(),
            Some(RepositoryAction::Fetch)
        );
        app.complete_operation(&OperationResult::Fetch { updated_refs: 4 }, snapshot());
        assert_eq!(app.toasts.len(), 3);
        assert_eq!(
            app.toasts
                .iter()
                .filter(|toast| toast.title == "Fetched 4 refs")
                .count(),
            1
        );
        let id = app.toasts[0].id;
        app.dismiss_toast(id);
        assert!(app.toasts.iter().all(|toast| toast.id != id));
    }
}
