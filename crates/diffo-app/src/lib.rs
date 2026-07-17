mod model;

use diffo_core::{RepositoryAction, RepositorySnapshot};

pub use model::{ChangeArea, FileKey, Model};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Message {
    Quit,
    SelectPreviousFile,
    SelectNextFile,
    SelectFirstFile,
    SelectLastFile,
    SelectFile(FileKey),
    ScrollDiffUp,
    ScrollDiffDown,
    ScrollDiffLeft,
    ScrollDiffRight,
    StageSelected,
    UnstageSelected,
    StageAll,
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
        Message::SelectPreviousFile => model.select_previous(),
        Message::SelectNextFile => model.select_next(),
        Message::SelectFirstFile => model.select_first(),
        Message::SelectLastFile => model.select_last(),
        Message::SelectFile(file) => model.select_file(&file),
        Message::ScrollDiffUp => model.scroll_diff_up(),
        Message::ScrollDiffDown => model.scroll_diff_down(),
        Message::ScrollDiffLeft => model.scroll_diff_left(),
        Message::ScrollDiffRight => model.scroll_diff_right(),
        Message::StageSelected => return model.stage_selected().map(Effect::Repository),
        Message::UnstageSelected => return model.unstage_selected().map(Effect::Repository),
        Message::StageAll => return model.stage_all().map(Effect::Repository),
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
    };

    use super::{ChangeArea, Effect, FileKey, Message, Model, update};

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
        assert_eq!(model.diff_horizontal_scroll, 1);
        assert_eq!(update(&mut model, Message::Quit), None);
        assert!(model.should_quit);
    }

    #[test]
    fn returns_repository_effect() {
        let mut model = model(AccessMode::ReadWrite);

        assert_eq!(
            update(&mut model, Message::StageSelected),
            Some(Effect::Repository(RepositoryAction::Stage(
                PathBuf::from("file.txt")
            )))
        );
    }

    #[test]
    fn read_only_model_returns_no_effect() {
        let mut model = model(AccessMode::ReadOnly);

        assert_eq!(update(&mut model, Message::StageSelected), None);
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
