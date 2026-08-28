use diffo_app::{history::HistoryRequest, workbench::Workbench};
use diffo_repository_service::{RepositoryEvent, RepositoryService};

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
            HistoryRequest::File {
                query_id,
                commit_id,
                path,
                old_path,
            } => {
                if !repository_service.load_commit_file(
                    query_id,
                    commit_id.clone(),
                    path.clone(),
                    old_path,
                ) {
                    workbench.commit_file_load_failed(
                        query_id,
                        &commit_id,
                        &path,
                        "repository service is unavailable",
                    );
                }
            }
        }
    }
}

pub(super) fn accept_event(
    workbench: &mut Workbench,
    event: RepositoryEvent,
) -> Option<RepositoryEvent> {
    match event {
        RepositoryEvent::HistoryLoaded { query_id, history } => {
            workbench.history_loaded(query_id, history);
        }
        RepositoryEvent::HistoryLoadFailed { query_id, message } => {
            workbench.history_load_failed(query_id, &message);
        }
        RepositoryEvent::CommitPatchLoaded {
            query_id,
            commit_id,
            patch,
            files,
        } => workbench.commit_patch_loaded(query_id, &commit_id, patch, files),
        RepositoryEvent::CommitPatchLoadFailed {
            query_id,
            commit_id,
            message,
        } => workbench.commit_patch_load_failed(query_id, &commit_id, &message),
        RepositoryEvent::CommitFileLoaded {
            query_id,
            commit_id,
            path,
            patch,
        } => workbench.commit_file_loaded(query_id, &commit_id, path, patch),
        RepositoryEvent::CommitFileLoadFailed {
            query_id,
            commit_id,
            path,
            message,
        } => workbench.commit_file_load_failed(query_id, &commit_id, &path, &message),
        event => return Some(event),
    }
    None
}
