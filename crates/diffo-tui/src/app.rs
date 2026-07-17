use std::path::{Path, PathBuf};

use diffo_core::{AccessMode, RepositoryAction, RepositorySnapshot};

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

pub struct App {
    pub snapshot: RepositorySnapshot,
    pub selected: Option<FileKey>,
    pub should_quit: bool,
    pub error: Option<String>,
    pub access_mode: AccessMode,
    cursor: usize,
}

impl App {
    #[must_use]
    pub fn new(snapshot: RepositorySnapshot, access_mode: AccessMode) -> Self {
        let selected = file_keys(&snapshot).into_iter().next();
        Self {
            snapshot,
            selected,
            should_quit: false,
            error: None,
            access_mode,
            cursor: 0,
        }
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
    pub fn unstage_selected(&self) -> Option<RepositoryAction> {
        if self.access_mode == AccessMode::ReadOnly {
            return None;
        }
        self.selected.as_ref().and_then(|key| {
            (key.area == ChangeArea::Staged).then(|| RepositoryAction::Unstage(key.path.clone()))
        })
    }

    #[must_use]
    pub fn stage_all(&self) -> Option<RepositoryAction> {
        if self.access_mode == AccessMode::ReadOnly {
            return None;
        }
        self.snapshot
            .files
            .iter()
            .any(|file| file.unstaged.is_some() || file.kind == diffo_core::ChangeKind::Untracked)
            .then_some(RepositoryAction::StageAll)
    }

    pub fn refresh(&mut self, snapshot: RepositorySnapshot) {
        let old_selected = self.selected.clone();
        let old_cursor = self.cursor;
        self.snapshot = snapshot;
        let keys = file_keys(&self.snapshot);

        self.cursor = old_selected
            .as_ref()
            .and_then(|selected| keys.iter().position(|key| key == selected))
            .unwrap_or_else(|| old_cursor.min(keys.len().saturating_sub(1)));
        self.selected = keys.get(self.cursor).cloned();
        self.error = None;
    }

    pub fn show_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    #[must_use]
    pub fn is_selected(&self, path: &Path, area: ChangeArea) -> bool {
        self.selected
            .as_ref()
            .is_some_and(|key| key.path == path && key.area == area)
    }

    #[must_use]
    pub fn selected_row(&self) -> Option<usize> {
        self.selected.as_ref().map(|selected| match selected.area {
            ChangeArea::Unstaged => self.cursor + 1,
            ChangeArea::Staged => self.cursor + 2,
        })
    }
}

fn file_keys(snapshot: &RepositorySnapshot) -> Vec<FileKey> {
    unstaged_files(snapshot)
        .map(|file| FileKey {
            path: file.path.clone(),
            area: ChangeArea::Unstaged,
        })
        .chain(staged_files(snapshot).map(|file| FileKey {
            path: file.path.clone(),
            area: ChangeArea::Staged,
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
        AccessMode, ChangeKind, FileDiff, FileState, RepositoryAction, RepositorySnapshot,
    };

    use super::{App, ChangeArea, FileKey};

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
        let mut app = App::new(snapshot(), AccessMode::ReadWrite);
        assert_eq!(
            app.selected.as_ref().expect("selection").path,
            PathBuf::from("both.txt")
        );

        app.select_next();
        assert_eq!(
            app.selected.as_ref().expect("selection").path,
            PathBuf::from("new.txt")
        );
        app.select_next();
        assert_eq!(
            app.selected.as_ref().expect("selection").area,
            ChangeArea::Staged
        );
    }

    #[test]
    fn creates_actions_for_the_selected_group() {
        let mut app = App::new(snapshot(), AccessMode::ReadWrite);
        assert_eq!(
            app.stage_selected(),
            Some(RepositoryAction::Stage(PathBuf::from("both.txt")))
        );
        assert_eq!(app.unstage_selected(), None);

        app.select_last();
        assert_eq!(
            app.unstage_selected(),
            Some(RepositoryAction::Unstage(PathBuf::from("both.txt")))
        );
    }

    #[test]
    fn keeps_selection_after_refresh() {
        let mut app = App::new(snapshot(), AccessMode::ReadWrite);
        app.select_last();
        let selected = FileKey {
            path: PathBuf::from("both.txt"),
            area: ChangeArea::Staged,
        };

        app.refresh(snapshot());

        assert_eq!(app.selected, Some(selected));
    }

    #[test]
    fn read_only_mode_blocks_actions() {
        let app = App::new(snapshot(), AccessMode::ReadOnly);

        assert_eq!(app.stage_selected(), None);
        assert_eq!(app.stage_all(), None);
    }
}
