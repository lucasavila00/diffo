use diffo_app::workbench::Workbench;
use diffo_repository_service::RepositoryService;

pub(super) fn dispatch_queries(workbench: &mut Workbench, repository_service: &RepositoryService) {
    while let Some(query_id) = workbench.take_merge_query() {
        if !repository_service.load_merge_refs(query_id) {
            workbench.merge_refs_load_failed(query_id, "repository service is unavailable");
        }
    }
}
