use diffo_core::{CheckoutHistory, RepositoryQueryId};

use super::{Activity, HistoryRequest, Workbench};

impl Workbench {
    pub fn take_history_request(&mut self) -> Option<HistoryRequest> {
        self.history.take_request()
    }

    pub fn history_loaded(&mut self, query_id: RepositoryQueryId, history: CheckoutHistory) {
        if self.history.accept_history(query_id, history) && self.active == Activity::History {
            self.request_redraw();
        }
    }

    pub fn history_load_failed(&mut self, query_id: RepositoryQueryId, message: &str) {
        if self.history.history_failed(query_id) {
            self.show_error("History refresh failed", message);
        }
    }

    pub fn commit_patch_loaded(
        &mut self,
        query_id: RepositoryQueryId,
        commit_id: String,
        patch: String,
    ) {
        if self.history.accept_patch(query_id, commit_id, patch) && self.active == Activity::History
        {
            self.request_redraw();
        }
    }

    pub fn commit_patch_load_failed(
        &mut self,
        query_id: RepositoryQueryId,
        commit_id: &str,
        message: &str,
    ) {
        if self.history.patch_failed(query_id, commit_id) {
            self.show_error("Could not open commit", message);
        }
    }
}
