use diffo_core::{Repository, RepositoryQueryId, RepositoryUpdate, RepositoryUpdateKind};

use crate::service::RepositoryEvent;

pub(super) fn history(repository: &dyn Repository, query_id: RepositoryQueryId) -> RepositoryEvent {
    match repository.checkout_history() {
        Ok(history) => RepositoryEvent::HistoryLoaded { query_id, history },
        Err(error) => RepositoryEvent::HistoryLoadFailed {
            query_id,
            message: error.to_string(),
        },
    }
}

pub(super) fn commit_patch(
    repository: &dyn Repository,
    query_id: RepositoryQueryId,
    commit_id: String,
) -> RepositoryEvent {
    match repository.commit_patch(&commit_id) {
        Ok(patch) => RepositoryEvent::CommitPatchLoaded {
            query_id,
            commit_id,
            patch,
        },
        Err(error) => RepositoryEvent::CommitPatchLoadFailed {
            query_id,
            commit_id,
            message: error.to_string(),
        },
    }
}

pub(super) fn branches(
    repository: &dyn Repository,
    query_id: RepositoryQueryId,
) -> RepositoryEvent {
    match repository.branches() {
        Ok(branches) => RepositoryEvent::BranchesLoaded { query_id, branches },
        Err(error) => RepositoryEvent::BranchesLoadFailed {
            query_id,
            message: error.to_string(),
        },
    }
}

pub(super) fn merge_refs(
    repository: &dyn Repository,
    query_id: RepositoryQueryId,
) -> RepositoryEvent {
    match repository.merge_refs() {
        Ok(refs) => RepositoryEvent::MergeRefsLoaded { query_id, refs },
        Err(error) => RepositoryEvent::MergeRefsLoadFailed {
            query_id,
            message: error.to_string(),
        },
    }
}

pub(super) fn stashes(repository: &dyn Repository, query_id: RepositoryQueryId) -> RepositoryEvent {
    match repository.stashes() {
        Ok(stashes) => RepositoryEvent::StashesLoaded { query_id, stashes },
        Err(error) => RepositoryEvent::StashesLoadFailed {
            query_id,
            message: error.to_string(),
        },
    }
}

pub(super) fn remotes(repository: &dyn Repository, query_id: RepositoryQueryId) -> RepositoryEvent {
    match repository.remotes() {
        Ok(remotes) => RepositoryEvent::RemotesLoaded { query_id, remotes },
        Err(error) => RepositoryEvent::RemotesLoadFailed {
            query_id,
            message: error.to_string(),
        },
    }
}

pub(super) fn refresh(repository: &dyn Repository, generation: &mut u64) -> RepositoryEvent {
    *generation = generation.saturating_add(1);
    match repository.snapshot() {
        Ok(snapshot) => RepositoryEvent::Update(RepositoryUpdate {
            generation: *generation,
            kind: RepositoryUpdateKind::Snapshot(snapshot),
        }),
        Err(error) => RepositoryEvent::Update(RepositoryUpdate {
            generation: *generation,
            kind: RepositoryUpdateKind::RefreshFailed(error.to_string()),
        }),
    }
}
