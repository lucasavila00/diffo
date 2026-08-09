//! Pure Diff state, messages, effects, and updates.

mod state;

use diffo_core::{OperationFailure, OperationResult, RepositoryAction, RepositorySnapshot};

#[cfg(test)]
pub(crate) use state::operation_result_toast;
pub use state::{
    ChangeArea, DiffViewMode, FileKey, Model, NetworkOperation, Toast, ToastKind, ToastQueue,
};
pub(crate) use state::{MergePhase, sync_plan_title, sync_progress_label};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Message {
    Quit,
    SelectFile(FileKey),
    ScrollDiffUp,
    ScrollDiffDown,
    ScrollDiffPageUp(usize),
    ScrollDiffPageDown(usize),
    ScrollDiffVerticalBy(i64),
    SetDiffScroll(usize),
    JumpDiffToPosition(usize),
    SetDiffHorizontalScroll(usize),
    ScrollDiffLeft,
    ScrollDiffRight,
    ScrollDiffHorizontalBy(i64),
    JumpToPreviousChange,
    JumpToNextChange,
    ToggleDiffView,
    BeginFilePaneResize,
    ResizeFilePane(u16),
    EndFilePaneResize,
    ToggleStageSelected,
    ToggleStageAll,
    StageAll,
    UnstageAll,
    StageFile(std::path::PathBuf),
    UnstageFile(std::path::PathBuf),
    RequestDiscardFile(std::path::PathBuf),
    FocusCommitInput,
    BlurCommitInput,
    CommitMessageInput(char),
    CommitMessageBackspace,
    CommitMessageCursorLeft,
    CommitMessageCursorRight,
    ExecuteCommit,
    ExecuteSync,
    ExecuteSyncToRemote(String),
    SnapshotLoaded(RepositorySnapshot),
    OperationFailed(String),
    OperationCompleted(RepositoryAction, OperationResult, Box<RepositorySnapshot>),
    OperationCancelled(RepositoryAction),
    ActionFailed(OperationFailure),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    Repository(RepositoryAction),
    Toast(ToastKind, String),
    Error(String, String),
}

pub fn update(model: &mut Model, message: Message) -> Option<Effect> {
    match message {
        Message::Quit => model.should_quit = true,
        Message::SelectFile(file) => model.select_file(&file),
        Message::ScrollDiffUp => model.scroll_diff_up(),
        Message::ScrollDiffDown => model.scroll_diff_down(),
        Message::ScrollDiffPageUp(lines) => model.scroll_diff_up_by(lines),
        Message::ScrollDiffPageDown(lines) => model.scroll_diff_down_by(lines),
        Message::ScrollDiffVerticalBy(lines) => model.scroll_diff_vertical_by(lines),
        Message::SetDiffScroll(position) => model.diff_scroll = position,
        Message::SetDiffHorizontalScroll(position) => model.diff_horizontal_scroll = position,
        Message::ScrollDiffLeft => model.scroll_diff_left(),
        Message::ScrollDiffRight => model.scroll_diff_right(),
        Message::ScrollDiffHorizontalBy(columns) => model.scroll_diff_horizontal_by(columns),
        Message::JumpDiffToPosition(_)
        | Message::JumpToPreviousChange
        | Message::JumpToNextChange
        | Message::FocusCommitInput
        | Message::BlurCommitInput
        | Message::RequestDiscardFile(_) => {}
        Message::ToggleDiffView => model.toggle_diff_view(),
        Message::BeginFilePaneResize => model.begin_file_pane_resize(),
        Message::ResizeFilePane(percent) => model.resize_file_pane(percent),
        Message::EndFilePaneResize => model.end_file_pane_resize(),
        Message::ToggleStageSelected => {
            return model.toggle_stage_selected().map(Effect::Repository);
        }
        Message::ToggleStageAll => return model.toggle_stage_all().map(Effect::Repository),
        Message::StageAll => return model.stage_all().map(Effect::Repository),
        Message::UnstageAll => return model.unstage_all().map(Effect::Repository),
        Message::StageFile(path) => return model.stage_file(path).map(Effect::Repository),
        Message::UnstageFile(path) => return model.unstage_file(path).map(Effect::Repository),
        Message::CommitMessageInput(character) => model.commit_message_input(character),
        Message::CommitMessageBackspace => model.commit_message_backspace(),
        Message::CommitMessageCursorLeft => model.commit_message_cursor_left(),
        Message::CommitMessageCursorRight => model.commit_message_cursor_right(),
        Message::ExecuteCommit => return model.execute_commit().map(Effect::Repository),
        Message::ExecuteSync => return model.execute_sync().map(Effect::Repository),
        Message::ExecuteSyncToRemote(remote) => {
            return model.execute_sync_to_remote(remote).map(Effect::Repository);
        }
        Message::SnapshotLoaded(snapshot) => model.repository_changed(snapshot),
        Message::OperationFailed(error) => {
            return Some(Effect::Error("Repository refresh failed".to_owned(), error));
        }
        Message::OperationCompleted(action, result, snapshot) => {
            if model.complete_operation(&action, &result, *snapshot)
                && let Some((kind, title)) = state::operation_result_toast(&result)
            {
                return Some(Effect::Toast(kind, title));
            }
        }
        Message::OperationCancelled(action) => {
            model.cancel_operation(&action);
        }
        Message::ActionFailed(failure) => {
            if model.fail_operation(&failure) {
                let (title, detail) = state::operation_failure_error(&failure);
                return Some(Effect::Error(title, detail));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests;
