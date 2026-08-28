use diffo_core::{CommitFile, RepositoryQueryId};

use super::{DiffViewMode, HistoryActivity, HistoryRequest, ReviewSelection};

impl HistoryActivity {
    fn next_id(&mut self) -> u64 {
        self.next_id = self.next_id.saturating_add(1);
        self.next_id
    }

    pub(super) fn request_history(&mut self) {
        let id = self.next_id();
        self.latest_history = id;
        self.history_pending = true;
        self.queued
            .retain(|request| !matches!(request, HistoryRequest::Commits { .. }));
        self.queued.push_back(HistoryRequest::Commits {
            query_id: RepositoryQueryId(id),
        });
    }

    pub(super) fn request_patch(&mut self, commit_id: String) {
        let id = self.next_id();
        self.latest_patch = id;
        self.pending_selection = Some(ReviewSelection::CompleteChange(commit_id.clone()));
        self.pending_document = None;
        self.pending_files = None;
        self.pending_hunks = None;
        self.pending_mode = Some(DiffViewMode::Hunk);
        self.patch_pending = true;
        self.file_pending = false;
        self.queued.retain(|request| {
            !matches!(
                request,
                HistoryRequest::Patch { .. } | HistoryRequest::File { .. }
            )
        });
        self.queued.push_back(HistoryRequest::Patch {
            query_id: RepositoryQueryId(id),
            commit_id,
        });
    }

    pub(super) fn request_file(&mut self, commit_id: String, file: &CommitFile) {
        let id = self.next_id();
        self.latest_file = id;
        self.pending_selection = Some(ReviewSelection::HistoryFile {
            commit_id: commit_id.clone(),
            path: file.path.clone(),
        });
        self.pending_document = None;
        self.file_pending = true;
        self.queued
            .retain(|request| !matches!(request, HistoryRequest::File { .. }));
        self.queued.push_back(HistoryRequest::File {
            query_id: RepositoryQueryId(id),
            commit_id,
            path: file.path.clone(),
            old_path: file.old_path.clone(),
        });
    }

    pub(super) fn supersede_pending_selection(&mut self, selection: &ReviewSelection) {
        let Some(pending) = self.pending_selection.as_ref() else {
            return;
        };
        if pending == selection {
            return;
        }
        let changes_commit =
            super::selection_commit_id(pending) != super::selection_commit_id(selection);
        self.file_pending = false;
        self.queued
            .retain(|request| !matches!(request, HistoryRequest::File { .. }));
        if !changes_commit {
            return;
        }
        self.patch_pending = false;
        self.queued
            .retain(|request| !matches!(request, HistoryRequest::Patch { .. }));
        self.pending_commits = None;
        self.pending_document = None;
        self.pending_files = None;
        self.pending_hunks = None;
        self.pending_mode = None;
    }
}
