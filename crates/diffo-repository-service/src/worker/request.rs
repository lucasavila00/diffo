use std::path::PathBuf;

use diffo_core::{ApplicationCommandId, CancellationHandle, RepositoryAction, RepositoryQueryId};

pub(crate) enum WorkerRequest {
    RefreshRequested,
    LoadHistory {
        query_id: RepositoryQueryId,
    },
    LoadCommitPatch {
        query_id: RepositoryQueryId,
        commit_id: String,
    },
    LoadCommitFile {
        query_id: RepositoryQueryId,
        commit_id: String,
        path: PathBuf,
        old_path: Option<PathBuf>,
    },
    LoadBranches {
        query_id: RepositoryQueryId,
    },
    LoadMergeRefs {
        query_id: RepositoryQueryId,
    },
    LoadStashes {
        query_id: RepositoryQueryId,
    },
    LoadRemotes {
        query_id: RepositoryQueryId,
    },
    Execute {
        id: ApplicationCommandId,
        action: RepositoryAction,
        cancellation: CancellationHandle,
    },
    WatchFailed(String),
    Shutdown,
}

pub(super) enum DebouncedRequest {
    Refresh,
    LoadHistory {
        query_id: RepositoryQueryId,
    },
    LoadCommitPatch {
        query_id: RepositoryQueryId,
        commit_id: String,
    },
    LoadCommitFile {
        query_id: RepositoryQueryId,
        commit_id: String,
        path: PathBuf,
        old_path: Option<PathBuf>,
    },
    LoadBranches {
        query_id: RepositoryQueryId,
    },
    LoadMergeRefs {
        query_id: RepositoryQueryId,
    },
    LoadStashes {
        query_id: RepositoryQueryId,
    },
    LoadRemotes {
        query_id: RepositoryQueryId,
    },
    Execute {
        id: ApplicationCommandId,
        action: RepositoryAction,
        cancellation: CancellationHandle,
    },
    Shutdown,
}
