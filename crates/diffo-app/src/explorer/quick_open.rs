use std::path::{Path, PathBuf};

use super::{EntryId, ExplorerActivity, ExplorerRequest};

impl ExplorerActivity {
    pub(crate) fn request_quick_open_paths(&mut self) {
        let id = self.next_id();
        self.latest_quick_open_paths = id;
        self.quick_open_paths_pending = true;
        self.quick_open_paths.clear();
        self.queued
            .retain(|request| !matches!(request, ExplorerRequest::QuickOpenPaths { .. }));
        self.queued
            .push_back(ExplorerRequest::QuickOpenPaths { id });
    }

    pub(crate) fn quick_open_paths(&self) -> (&[PathBuf], bool) {
        (&self.quick_open_paths, self.quick_open_paths_pending)
    }

    pub(super) fn accept_quick_open_paths(
        &mut self,
        result: Result<Vec<PathBuf>, String>,
    ) -> (Option<(String, String)>, bool) {
        self.quick_open_paths_pending = false;
        match result {
            Ok(mut paths) => {
                paths.sort();
                paths.dedup();
                let changed = self.quick_open_paths != paths;
                self.quick_open_paths = paths;
                (None, changed)
            }
            Err(error) => (Some(("Quick Open refresh failed".to_owned(), error)), true),
        }
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
