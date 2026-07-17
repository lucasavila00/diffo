use std::path::{Path, PathBuf};

use diffo_core::{AccessMode, RepositoryAction, RepositorySnapshot};

use crate::CommandPalette;

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
pub enum DiffViewMode {
    #[default]
    Inline,
    SideBySide,
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
    expanded_file_pane_percent: u16,
    cursor: usize,
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
            expanded_file_pane_percent: 25,
            cursor: 0,
        }
    }

    pub fn open_command_palette(&mut self) {
        self.command_palette = Some(CommandPalette::default());
    }

    pub fn close_command_palette(&mut self) {
        self.command_palette = None;
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

    pub fn select_next(&mut self) {
        let keys = file_keys(&self.snapshot);
        if keys.is_empty() {
            return;
        }
        self.cursor = self.cursor.saturating_add(1).min(keys.len() - 1);
        self.selected = keys.get(self.cursor).cloned();
        self.reset_diff_scroll();
        self.error = None;
    }

    pub fn select_previous(&mut self) {
        let keys = file_keys(&self.snapshot);
        self.cursor = self.cursor.saturating_sub(1);
        self.selected = keys.get(self.cursor).cloned();
        self.reset_diff_scroll();
        self.error = None;
    }

    pub fn select_first(&mut self) {
        self.cursor = 0;
        self.selected = file_keys(&self.snapshot).into_iter().next();
        self.reset_diff_scroll();
    }

    pub fn select_last(&mut self) {
        let keys = file_keys(&self.snapshot);
        self.cursor = keys.len().saturating_sub(1);
        self.selected = keys.get(self.cursor).cloned();
        self.reset_diff_scroll();
    }

    pub fn select_file(&mut self, file: &FileKey) {
        let keys = file_keys(&self.snapshot);
        if let Some(cursor) = keys.iter().position(|key| key == file) {
            self.cursor = cursor;
            self.selected = keys.get(cursor).cloned();
            self.reset_diff_scroll();
            self.error = None;
        }
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

    pub fn scroll_diff_right(&mut self) {
        self.diff_horizontal_scroll = self.diff_horizontal_scroll.saturating_add(4);
    }

    pub fn scroll_diff_left(&mut self) {
        self.diff_horizontal_scroll = self.diff_horizontal_scroll.saturating_sub(4);
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
    pub fn toggle_stage_selected(&self) -> Option<RepositoryAction> {
        match self.selected.as_ref()?.area {
            ChangeArea::Unstaged => self.stage_selected(),
            ChangeArea::Staged => self.unstage_selected(),
        }
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
        self.reset_diff_scroll();
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

    fn reset_diff_scroll(&mut self) {
        self.diff_scroll = 0;
        self.diff_horizontal_scroll = 0;
    }
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
        AccessMode, ChangeKind, FileDiff, FileState, RepositoryAction, RepositorySnapshot,
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
    fn keeps_selection_after_refresh() {
        let mut app = Model::new(snapshot(), AccessMode::ReadWrite);
        let selected = FileKey {
            path: PathBuf::from("both.txt"),
            area: ChangeArea::Staged,
        };

        app.refresh(snapshot());

        assert_eq!(app.selected, Some(selected));
    }

    #[test]
    fn read_only_mode_blocks_actions() {
        let app = Model::new(snapshot(), AccessMode::ReadOnly);

        assert_eq!(app.stage_selected(), None);
        assert_eq!(app.toggle_stage_all(), None);
    }
}
