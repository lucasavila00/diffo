mod model;

use diffo_core::{OperationFailure, OperationResult, RepositoryAction, RepositorySnapshot};

pub use model::{
    ChangeArea, DiffViewMode, FileContextMenu, FileKey, FileListScroll, Model, NetworkOperation,
    PrimaryAction, Toast, ToastKind,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Message {
    Quit,
    ToggleHelp,
    CloseHelp,
    SelectPreviousFile,
    SelectNextFile,
    SelectFirstFile,
    SelectLastFile,
    SelectFile(FileKey),
    OpenFileContextMenu(FileKey, u16, u16),
    CloseFileContextMenu,
    CopyRelativePath,
    CopyAbsolutePath,
    ScrollDiffUp,
    ScrollDiffDown,
    ScrollDiffPageUp(usize),
    ScrollDiffPageDown(usize),
    ScrollDiffBy(i64),
    SetDiffScroll(usize),
    SetDiffHorizontalScroll(usize),
    ScrollDiffLeft,
    ScrollDiffRight,
    ScrollDiffHorizontalBy(i64),
    ScrollFileListBy(ChangeArea, i64),
    SetFileListScroll(ChangeArea, usize),
    JumpToPreviousChange,
    JumpToNextChange,
    ToggleDiffView,
    ToggleFilePane,
    BeginFilePaneResize,
    ResizeFilePane(u16),
    EndFilePaneResize,
    ToggleStageSelected,
    ToggleStageAll,
    StageAll,
    UnstageAll,
    StageFile(std::path::PathBuf),
    UnstageFile(std::path::PathBuf),
    FocusCommitInput,
    BlurCommitInput,
    CommitMessageInput(char),
    CommitMessageBackspace,
    CommitMessageCursorLeft,
    CommitMessageCursorRight,
    ExecutePrimaryAction,
    SnapshotLoaded(RepositorySnapshot),
    OperationFailed(String),
    OperationCompleted(RepositoryAction, OperationResult, RepositorySnapshot),
    ActionFailed(OperationFailure),
    DismissToast(u64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    Repository(RepositoryAction),
    CopyPath {
        path: std::path::PathBuf,
        absolute: bool,
    },
}

pub fn update(model: &mut Model, message: Message) -> Option<Effect> {
    match message {
        Message::Quit => model.should_quit = true,
        Message::ToggleHelp => model.toggle_help(),
        Message::CloseHelp => model.close_help(),
        Message::SelectPreviousFile => model.select_previous(),
        Message::SelectNextFile => model.select_next(),
        Message::SelectFirstFile => model.select_first(),
        Message::SelectLastFile => model.select_last(),
        Message::SelectFile(file) => model.select_file(&file),
        Message::OpenFileContextMenu(file, column, row) => {
            model.open_file_context_menu(file, column, row);
        }
        Message::CloseFileContextMenu => model.close_file_context_menu(),
        Message::CopyRelativePath => {
            return model.copy_context_path(false);
        }
        Message::CopyAbsolutePath => {
            return model.copy_context_path(true);
        }
        Message::ScrollDiffUp => model.scroll_diff_up(),
        Message::ScrollDiffDown => model.scroll_diff_down(),
        Message::ScrollDiffPageUp(lines) => model.scroll_diff_up_by(lines),
        Message::ScrollDiffPageDown(lines) => model.scroll_diff_down_by(lines),
        Message::ScrollDiffBy(lines) => model.scroll_diff_by(lines),
        Message::SetDiffScroll(position) => model.diff_scroll = position,
        Message::SetDiffHorizontalScroll(position) => model.diff_horizontal_scroll = position,
        Message::ScrollDiffLeft => model.scroll_diff_left(),
        Message::ScrollDiffRight => model.scroll_diff_right(),
        Message::ScrollDiffHorizontalBy(columns) => model.scroll_diff_horizontal_by(columns),
        Message::ScrollFileListBy(area, rows) => model.scroll_file_list_by(area, rows),
        Message::SetFileListScroll(area, position) => model.set_file_list_scroll(area, position),
        Message::JumpToPreviousChange | Message::JumpToNextChange => {}
        Message::ToggleDiffView => model.toggle_diff_view(),
        Message::ToggleFilePane => model.toggle_file_pane(),
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
        Message::FocusCommitInput => model.focus_commit_input(),
        Message::BlurCommitInput => model.blur_commit_input(),
        Message::CommitMessageInput(character) => model.commit_message_input(character),
        Message::CommitMessageBackspace => model.commit_message_backspace(),
        Message::CommitMessageCursorLeft => model.commit_message_cursor_left(),
        Message::CommitMessageCursorRight => model.commit_message_cursor_right(),
        Message::ExecutePrimaryAction => {
            return model.execute_primary_action().map(Effect::Repository);
        }
        Message::SnapshotLoaded(snapshot) => model.repository_changed(snapshot),
        Message::OperationFailed(error) => model.show_error(error),
        Message::OperationCompleted(action, result, snapshot) => {
            model.complete_operation(&action, &result, snapshot);
        }
        Message::ActionFailed(failure) => model.show_operation_failure(&failure),
        Message::DismissToast(id) => model.dismiss_toast(id),
    }
    None
}

#[cfg(test)]
mod tests;
