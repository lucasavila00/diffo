mod command_palette;
mod model;

use diffo_core::{RepositoryAction, RepositorySnapshot};

pub use command_palette::{Command, CommandId, CommandPalette};
pub use model::{ChangeArea, DiffViewMode, FileKey, Model, PrimaryAction};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Message {
    Quit,
    OpenCommandPalette,
    CloseCommandPalette,
    ToggleHelp,
    CloseHelp,
    CommandPaletteInput(char),
    CommandPaletteBackspace,
    CommandPaletteSelectPrevious,
    CommandPaletteSelectNext,
    CommandPaletteSelect(usize),
    ExecuteCommand(usize),
    ExecuteSelectedCommand,
    SelectPreviousFile,
    SelectNextFile,
    SelectFirstFile,
    SelectLastFile,
    SelectFile(FileKey),
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
    ExecutePrimaryAction,
    SnapshotLoaded(RepositorySnapshot),
    OperationFailed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    Repository(RepositoryAction),
}

pub fn update(model: &mut Model, message: Message) -> Option<Effect> {
    match message {
        Message::Quit => model.should_quit = true,
        Message::OpenCommandPalette => model.open_command_palette(),
        Message::CloseCommandPalette => model.close_command_palette(),
        Message::ToggleHelp => model.toggle_help(),
        Message::CloseHelp => model.close_help(),
        Message::CommandPaletteInput(character) => model.command_palette_input(character),
        Message::CommandPaletteBackspace => model.command_palette_backspace(),
        Message::CommandPaletteSelectPrevious => model.command_palette_select_previous(),
        Message::CommandPaletteSelectNext => model.command_palette_select_next(),
        Message::CommandPaletteSelect(index) => model.command_palette_select(index),
        Message::ExecuteCommand(index) => {
            model.command_palette_select(index);
            return model.execute_selected_command().map(Effect::Repository);
        }
        Message::ExecuteSelectedCommand => {
            return model.execute_selected_command().map(Effect::Repository);
        }
        Message::SelectPreviousFile => model.select_previous(),
        Message::SelectNextFile => model.select_next(),
        Message::SelectFirstFile => model.select_first(),
        Message::SelectLastFile => model.select_last(),
        Message::SelectFile(file) => model.select_file(&file),
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
        Message::ExecutePrimaryAction => {
            return model.execute_primary_action().map(Effect::Repository);
        }
        Message::SnapshotLoaded(snapshot) => model.refresh(snapshot),
        Message::OperationFailed(error) => model.show_error(error),
    }
    None
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use diffo_core::{
        AccessMode, ChangeKind, FileDiff, FileState, RepositoryAction, RepositorySnapshot,
        UpstreamState,
    };

    use super::{ChangeArea, DiffViewMode, Effect, FileKey, Message, Model, PrimaryAction, update};

    fn model(access_mode: AccessMode) -> Model {
        Model::new(
            RepositorySnapshot {
                files: vec![FileState {
                    path: PathBuf::from("file.txt"),
                    old_path: None,
                    kind: ChangeKind::Modified,
                    staged: Some(FileDiff {
                        text: String::new(),
                    }),
                    unstaged: Some(FileDiff {
                        text: String::new(),
                    }),
                }],
                ..RepositorySnapshot::default()
            },
            access_mode,
        )
    }

    #[test]
    fn updates_local_state() {
        let mut model = model(AccessMode::ReadWrite);

        assert_eq!(update(&mut model, Message::ScrollDiffRight), None);
        assert_eq!(model.diff_horizontal_scroll, 4);
        assert_eq!(update(&mut model, Message::Quit), None);
        assert!(model.should_quit);
    }

    #[test]
    fn primary_action_chooses_commit_push_pull_or_blocked_sync() {
        let mut model = model(AccessMode::ReadWrite);
        assert_eq!(model.primary_action(), PrimaryAction::Disabled);

        update(&mut model, Message::FocusCommitInput);
        for character in "ship it".chars() {
            update(&mut model, Message::CommitMessageInput(character));
        }
        assert_eq!(model.primary_action(), PrimaryAction::Commit);
        assert_eq!(
            update(&mut model, Message::ExecutePrimaryAction),
            Some(Effect::Repository(RepositoryAction::Commit(
                "ship it".to_owned()
            )))
        );
        assert_eq!(model.commit_message, "ship it");
        let refreshed = model.snapshot.clone();
        update(&mut model, Message::SnapshotLoaded(refreshed));
        assert!(model.commit_message.is_empty());

        model.snapshot.files[0].staged = None;
        model.snapshot.upstream = Some(UpstreamState {
            name: "origin/main".to_owned(),
            ahead: 1,
            behind: 0,
        });
        assert_eq!(model.primary_action(), PrimaryAction::Push);
        model.snapshot.upstream.as_mut().unwrap().ahead = 0;
        model.snapshot.upstream.as_mut().unwrap().behind = 1;
        assert_eq!(model.primary_action(), PrimaryAction::Pull);
        model.snapshot.upstream.as_mut().unwrap().ahead = 1;
        assert_eq!(model.primary_action(), PrimaryAction::PushAndPull);
        assert_eq!(model.primary_action().label(), "Push + Pull");
        assert!(!model.primary_action().enabled());
        assert_eq!(update(&mut model, Message::ExecutePrimaryAction), None);
    }

    #[test]
    fn edits_and_closes_command_palette_state() {
        let mut model = model(AccessMode::ReadWrite);

        update(&mut model, Message::OpenCommandPalette);
        update(&mut model, Message::CommandPaletteInput('f'));
        update(&mut model, Message::CommandPaletteInput('p'));
        assert_eq!(
            model
                .command_palette
                .as_ref()
                .map(|palette| palette.query.as_str()),
            Some("fp")
        );
        update(&mut model, Message::CommandPaletteSelectNext);
        update(&mut model, Message::CommandPaletteBackspace);
        assert_eq!(model.command_palette.as_ref().unwrap().query, "f");
        update(&mut model, Message::CommandPaletteSelect(1));
        assert_eq!(model.command_palette.as_ref().unwrap().selected, 0);
        update(&mut model, Message::CloseCommandPalette);
        assert!(model.command_palette.is_none());
        assert!(!model.should_quit);
    }

    #[test]
    fn executes_fetch_and_pull_from_the_palette() {
        let mut model = model(AccessMode::ReadWrite);
        update(&mut model, Message::OpenCommandPalette);
        assert_eq!(
            update(&mut model, Message::ExecuteSelectedCommand),
            Some(Effect::Repository(RepositoryAction::Fetch))
        );
        assert!(model.command_palette.is_none());

        update(&mut model, Message::OpenCommandPalette);
        update(&mut model, Message::CommandPaletteSelectNext);
        assert_eq!(
            update(&mut model, Message::ExecuteSelectedCommand),
            Some(Effect::Repository(RepositoryAction::Pull))
        );

        update(&mut model, Message::OpenCommandPalette);
        assert_eq!(
            update(&mut model, Message::ExecuteCommand(1)),
            Some(Effect::Repository(RepositoryAction::Pull))
        );
    }

    #[test]
    fn help_is_a_toggle_and_closes_the_palette() {
        let mut model = model(AccessMode::ReadWrite);
        update(&mut model, Message::OpenCommandPalette);
        update(&mut model, Message::ToggleHelp);
        assert!(model.help_open);
        assert!(model.command_palette.is_none());
        update(&mut model, Message::ToggleHelp);
        assert!(!model.help_open);
    }

    #[test]
    fn scrolls_four_lines_in_the_arrow_direction() {
        let mut model = model(AccessMode::ReadWrite);

        update(&mut model, Message::ScrollDiffDown);
        assert_eq!(model.diff_scroll, 4);
        update(&mut model, Message::ScrollDiffUp);
        assert_eq!(model.diff_scroll, 0);
        update(&mut model, Message::ScrollDiffUp);
        assert_eq!(model.diff_scroll, 0);
    }

    #[test]
    fn scrolls_by_a_page() {
        let mut model = model(AccessMode::ReadWrite);

        update(&mut model, Message::ScrollDiffPageDown(27));
        assert_eq!(model.diff_scroll, 27);
        update(&mut model, Message::ScrollDiffPageUp(27));
        assert_eq!(model.diff_scroll, 0);
    }

    #[test]
    fn resizes_and_toggles_the_file_pane() {
        let mut model = model(AccessMode::ReadWrite);

        update(&mut model, Message::BeginFilePaneResize);
        update(&mut model, Message::ResizeFilePane(64));
        update(&mut model, Message::EndFilePaneResize);
        assert_eq!(model.file_pane_percent, 64);
        assert!(!model.resizing_file_pane);

        update(&mut model, Message::ToggleFilePane);
        assert_eq!(model.file_pane_percent, 0);
        update(&mut model, Message::ToggleFilePane);
        assert_eq!(model.file_pane_percent, 64);

        update(&mut model, Message::BeginFilePaneResize);
        update(&mut model, Message::ResizeFilePane(100));
        assert_eq!(model.file_pane_percent, 80);
    }

    #[test]
    fn toggles_diff_view_mode() {
        let mut model = model(AccessMode::ReadWrite);
        assert_eq!(model.diff_view_mode, DiffViewMode::Inline);
        model.scroll_diff_down();
        model.scroll_diff_right();

        update(&mut model, Message::ToggleDiffView);
        assert_eq!(model.diff_view_mode, DiffViewMode::SideBySide);
        assert_eq!(model.diff_scroll, 0);
        assert_eq!(model.diff_horizontal_scroll, 0);

        update(&mut model, Message::ToggleDiffView);
        assert_eq!(model.diff_view_mode, DiffViewMode::Inline);
    }

    #[test]
    fn returns_contextual_stage_effect() {
        let mut model = model(AccessMode::ReadWrite);

        assert_eq!(
            update(&mut model, Message::ToggleStageSelected),
            Some(Effect::Repository(RepositoryAction::Unstage(
                PathBuf::from("file.txt")
            )))
        );
        update(&mut model, Message::SelectNextFile);

        assert_eq!(
            update(&mut model, Message::ToggleStageSelected),
            Some(Effect::Repository(RepositoryAction::Stage(PathBuf::from(
                "file.txt"
            ))))
        );
    }

    #[test]
    fn returns_file_button_effects_without_changing_selection() {
        let mut model = model(AccessMode::ReadWrite);
        assert_eq!(
            update(&mut model, Message::StageFile(PathBuf::from("file.txt"))),
            Some(Effect::Repository(RepositoryAction::Stage(PathBuf::from(
                "file.txt"
            ))))
        );
        assert_eq!(
            update(&mut model, Message::UnstageFile(PathBuf::from("file.txt"))),
            Some(Effect::Repository(RepositoryAction::Unstage(
                PathBuf::from("file.txt")
            )))
        );
    }

    #[test]
    fn toggles_all_changes_contextually() {
        let mut model = model(AccessMode::ReadWrite);
        assert_eq!(
            update(&mut model, Message::ToggleStageAll),
            Some(Effect::Repository(RepositoryAction::StageAll))
        );

        model.snapshot.files[0].unstaged = None;
        assert_eq!(
            update(&mut model, Message::ToggleStageAll),
            Some(Effect::Repository(RepositoryAction::UnstageAll))
        );

        assert_eq!(
            update(&mut model, Message::UnstageAll),
            Some(Effect::Repository(RepositoryAction::UnstageAll))
        );
    }

    #[test]
    fn read_only_model_returns_no_effect() {
        let mut model = model(AccessMode::ReadOnly);

        assert_eq!(update(&mut model, Message::ToggleStageSelected), None);
    }

    #[test]
    fn selects_semantic_file_key() {
        let mut model = model(AccessMode::ReadWrite);
        let staged = FileKey {
            path: PathBuf::from("file.txt"),
            area: ChangeArea::Staged,
        };

        update(&mut model, Message::SelectFile(staged.clone()));

        assert_eq!(model.selected, Some(staged));
    }

    #[test]
    fn handles_runtime_results_as_messages() {
        let mut model = model(AccessMode::ReadWrite);
        let snapshot = RepositorySnapshot::default();

        update(&mut model, Message::SnapshotLoaded(snapshot.clone()));
        assert_eq!(model.snapshot, snapshot);
        update(
            &mut model,
            Message::OperationFailed("action failed".to_owned()),
        );
        assert_eq!(model.error.as_deref(), Some("action failed"));
    }
}
