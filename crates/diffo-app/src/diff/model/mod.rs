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
    ExecuteAiCommit,
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
        message @ (Message::ScrollDiffUp
        | Message::ScrollDiffDown
        | Message::ScrollDiffPageUp(_)
        | Message::ScrollDiffPageDown(_)
        | Message::ScrollDiffVerticalBy(_)
        | Message::SetDiffScroll(_)
        | Message::SetDiffHorizontalScroll(_)
        | Message::ScrollDiffLeft
        | Message::ScrollDiffRight
        | Message::ScrollDiffHorizontalBy(_)
        | Message::JumpDiffToPosition(_)
        | Message::JumpToPreviousChange
        | Message::JumpToNextChange
        | Message::ToggleDiffView) => {
            model.review.update(&message);
        }
        Message::FocusCommitInput
        | Message::BlurCommitInput
        | Message::ExecuteAiCommit
        | Message::RequestDiscardFile(_) => {}
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
            if model.complete_operation(&action, &result, *snapshot) {
                if let (RepositoryAction::GuardedCommit(target), OperationResult::Commit { hash }) =
                    (&action, &result)
                {
                    let short_hash = hash.get(..7.min(hash.len())).unwrap_or(hash);
                    return Some(Effect::Toast(
                        ToastKind::Success,
                        format!("Committed {short_hash} — {}", target.message),
                    ));
                }
                if let Some((kind, title)) = state::operation_result_toast(&result) {
                    return Some(Effect::Toast(kind, title));
                }
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
