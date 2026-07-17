use diffo_core::RepositoryAction;
use ratatui::layout::Rect;

use crate::{App, select_file_at};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiAction {
    Quit,
    SelectPreviousFile,
    SelectNextFile,
    SelectFirstFile,
    SelectLastFile,
    ScrollDiffUp,
    ScrollDiffDown,
    ScrollDiffLeft,
    ScrollDiffRight,
    SelectAt { column: u16, row: u16 },
    StageSelected,
    UnstageSelected,
    StageAll,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    Repository(RepositoryAction),
}

pub fn dispatch(app: &mut App, action: UiAction, area: Rect) -> Option<Effect> {
    match action {
        UiAction::Quit => app.should_quit = true,
        UiAction::SelectPreviousFile => app.select_previous(),
        UiAction::SelectNextFile => app.select_next(),
        UiAction::SelectFirstFile => app.select_first(),
        UiAction::SelectLastFile => app.select_last(),
        UiAction::ScrollDiffUp => app.scroll_diff_up(),
        UiAction::ScrollDiffDown => app.scroll_diff_down(),
        UiAction::ScrollDiffLeft => app.scroll_diff_left(),
        UiAction::ScrollDiffRight => app.scroll_diff_right(),
        UiAction::SelectAt { column, row } => select_file_at(app, area, column, row),
        UiAction::StageSelected => return app.stage_selected().map(Effect::Repository),
        UiAction::UnstageSelected => return app.unstage_selected().map(Effect::Repository),
        UiAction::StageAll => return app.stage_all().map(Effect::Repository),
    }
    None
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use diffo_core::{
        AccessMode, ChangeKind, FileDiff, FileState, RepositoryAction, RepositorySnapshot,
    };
    use ratatui::layout::Rect;

    use super::{Effect, UiAction, dispatch};
    use crate::{App, ChangeArea};

    fn app(access_mode: AccessMode) -> App {
        App::new(
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
    fn dispatches_local_state_changes() {
        let mut app = app(AccessMode::ReadWrite);
        let area = Rect::new(0, 0, 100, 30);

        assert_eq!(dispatch(&mut app, UiAction::ScrollDiffRight, area), None);
        assert_eq!(app.diff_horizontal_scroll, 1);
        assert_eq!(dispatch(&mut app, UiAction::Quit, area), None);
        assert!(app.should_quit);
    }

    #[test]
    fn returns_repository_effects() {
        let mut app = app(AccessMode::ReadWrite);

        let effect = dispatch(&mut app, UiAction::StageSelected, Rect::new(0, 0, 100, 30));

        assert_eq!(
            effect,
            Some(Effect::Repository(RepositoryAction::Stage(PathBuf::from(
                "file.txt"
            ))))
        );
    }

    #[test]
    fn read_only_mode_returns_no_repository_effect() {
        let mut app = app(AccessMode::ReadOnly);

        let effect = dispatch(&mut app, UiAction::StageSelected, Rect::new(0, 0, 100, 30));

        assert_eq!(effect, None);
    }

    #[test]
    fn dispatches_mouse_selection_and_ignores_header() {
        let mut app = app(AccessMode::ReadWrite);
        app.select_last();
        assert_eq!(
            app.selected.as_ref().expect("selection").area,
            ChangeArea::Staged
        );
        let area = Rect::new(0, 0, 100, 30);

        dispatch(&mut app, UiAction::SelectAt { column: 4, row: 1 }, area);
        assert_eq!(
            app.selected.as_ref().expect("selection").area,
            ChangeArea::Staged
        );

        dispatch(&mut app, UiAction::SelectAt { column: 4, row: 2 }, area);
        assert_eq!(
            app.selected.as_ref().expect("selection").area,
            ChangeArea::Unstaged
        );
    }
}
