use super::*;

impl Model {
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
    pub fn is_selected(&self, path: &Path, area: ChangeArea) -> bool {
        self.selected
            .as_ref()
            .is_some_and(|key| key.path == path && key.area == area)
    }

    pub(super) fn reset_diff_scroll(&mut self) {
        self.diff_scroll = 0;
        self.diff_horizontal_scroll = 0;
    }
}

pub(super) fn file_keys(snapshot: &RepositorySnapshot) -> Vec<FileKey> {
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
