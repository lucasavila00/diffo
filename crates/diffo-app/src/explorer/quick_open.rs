use std::path::{Path, PathBuf};

use super::{EntryId, ExplorerActivity};

impl ExplorerActivity {
    pub(crate) fn quick_open_paths(&self) -> (&[PathBuf], bool) {
        (&self.quick_open_paths, self.quick_open_paths_pending)
    }

    pub(crate) fn quick_open(&mut self, path: PathBuf) {
        if self.model.file_entry(&path).is_none() {
            self.request_paths();
            return;
        }
        self.pending_quick_open = Some(path.clone());
        self.request_file_load(path, 0);
    }

    pub(super) fn commit_quick_open_selection(&mut self, path: &Path) {
        let ancestors = path
            .ancestors()
            .skip(1)
            .filter(|ancestor| !ancestor.as_os_str().is_empty())
            .map(|ancestor| EntryId::Directory(ancestor.to_path_buf()))
            .collect::<Vec<_>>();
        self.picker
            .select_and_reveal(EntryId::File(path.to_path_buf()), ancestors);
    }
}
