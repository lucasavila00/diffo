use super::{ChangeArea, FileKey, Model, Path, RepositorySnapshot};

impl Model {
    pub fn select_next(&mut self) {
        let keys = file_keys(&self.snapshot);
        if keys.is_empty() {
            return;
        }
        self.cursor = self.cursor.saturating_add(1).min(keys.len() - 1);
        self.selected = keys.get(self.cursor).cloned();
    }

    pub fn select_previous(&mut self) {
        let keys = file_keys(&self.snapshot);
        self.cursor = self.cursor.saturating_sub(1);
        self.selected = keys.get(self.cursor).cloned();
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
        }
    }

    pub fn begin_file_pane_resize(&mut self) {
        self.resizing_file_pane = true;
    }

    pub fn resize_file_pane(&mut self, percent: u16) {
        if self.resizing_file_pane {
            self.file_pane_percent = percent.min(80);
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
