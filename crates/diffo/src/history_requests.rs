use diffo_app::{history::HistoryRequest, workbench::Workbench};
use diffo_repository_service::RepositoryService;

pub(super) fn dispatch(workbench: &mut Workbench, repository_service: &RepositoryService) {
    while let Some(request) = workbench.take_history_request() {
        match request {
            HistoryRequest::Commits { query_id } => {
                if !repository_service.load_history(query_id) {
                    workbench.history_load_failed(query_id, "repository service is unavailable");
                }
            }
            HistoryRequest::Patch {
                query_id,
                commit_id,
            } => {
                if !repository_service.load_commit_patch(query_id, commit_id.clone()) {
                    workbench.commit_patch_load_failed(
                        query_id,
                        &commit_id,
                        "repository service is unavailable",
                    );
                }
            }
        }
    }
}
