use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::Duration,
};

use diffo_core::{
    ApplicationCommandId, CancellationHandle, Repository, RepositoryAction, RepositoryQueryId,
    RepositoryUpdate, RepositoryUpdateKind,
};

use crate::service::{PromptBroker, RepositoryEvent};

mod operation;
mod queries;

use operation::execute as execute_command;
use queries::{
    branches as collect_branches, commit_patch as collect_commit_patch, history as collect_history,
    merge_refs as collect_merge_refs, refresh as collect_refresh, remotes as collect_remotes,
    stashes as collect_stashes,
};

#[cfg(test)]
use anyhow::Result;
#[cfg(test)]
use diffo_core::{
    OperationFailure, OperationOutcome, OperationResult, RepositoryOperationContext,
    RepositorySnapshot,
};
#[cfg(test)]
use std::thread;

const DEBOUNCE: Duration = Duration::from_millis(100);

pub(super) enum WorkerRequest {
    RefreshRequested,
    LoadHistory {
        query_id: RepositoryQueryId,
    },
    LoadCommitPatch {
        query_id: RepositoryQueryId,
        commit_id: String,
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

enum DebouncedRequest {
    Refresh,
    LoadHistory {
        query_id: RepositoryQueryId,
    },
    LoadCommitPatch {
        query_id: RepositoryQueryId,
        commit_id: String,
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

pub(super) fn worker_loop(
    repository: &dyn Repository,
    requests: &Receiver<WorkerRequest>,
    events: &Sender<RepositoryEvent>,
    refresh_pending: &AtomicBool,
    worktree_pending: &AtomicBool,
    busy: &AtomicBool,
    prompts: &Arc<PromptBroker>,
) {
    let mut generation = 0_u64;
    while let Ok(request) = requests.recv() {
        busy.store(true, Ordering::Release);
        let event = match request {
            WorkerRequest::RefreshRequested => {
                refresh_pending.store(false, Ordering::Release);
                let debounced = debounce(requests, refresh_pending);
                if worktree_pending.swap(false, Ordering::AcqRel)
                    && events.send(RepositoryEvent::WorktreeChanged).is_err()
                {
                    break;
                }
                let Some(event) =
                    collect_after_debounce(repository, debounced, &mut generation, prompts, events)
                else {
                    break;
                };
                Some(event)
            }
            WorkerRequest::Execute {
                id,
                action,
                cancellation,
            } => Some(execute_command(
                repository,
                id,
                &action,
                &cancellation,
                &mut generation,
                prompts,
                events,
            )),
            WorkerRequest::LoadBranches { query_id } => {
                Some(collect_branches(repository, query_id))
            }
            WorkerRequest::LoadHistory { query_id } => Some(collect_history(repository, query_id)),
            WorkerRequest::LoadCommitPatch {
                query_id,
                commit_id,
            } => Some(collect_commit_patch(repository, query_id, commit_id)),
            WorkerRequest::LoadMergeRefs { query_id } => {
                Some(collect_merge_refs(repository, query_id))
            }
            WorkerRequest::LoadStashes { query_id } => Some(collect_stashes(repository, query_id)),
            WorkerRequest::LoadRemotes { query_id } => Some(collect_remotes(repository, query_id)),
            WorkerRequest::WatchFailed(message) => {
                generation = generation.saturating_add(1);
                Some(RepositoryEvent::Update(RepositoryUpdate {
                    generation,
                    kind: RepositoryUpdateKind::RefreshFailed(format!(
                        "repository watch failed: {message}"
                    )),
                }))
            }
            WorkerRequest::Shutdown => break,
        };
        if let Some(event) = event
            && events.send(event).is_err()
        {
            break;
        }
        busy.store(false, Ordering::Release);
    }
    busy.store(false, Ordering::Release);
}

fn collect_after_debounce(
    repository: &dyn Repository,
    request: DebouncedRequest,
    generation: &mut u64,
    prompts: &Arc<PromptBroker>,
    events: &Sender<RepositoryEvent>,
) -> Option<RepositoryEvent> {
    let query = match request {
        DebouncedRequest::Refresh => return Some(collect_refresh(repository, generation)),
        DebouncedRequest::LoadHistory { query_id } => collect_history(repository, query_id),
        DebouncedRequest::LoadCommitPatch {
            query_id,
            commit_id,
        } => collect_commit_patch(repository, query_id, commit_id),
        DebouncedRequest::LoadBranches { query_id } => collect_branches(repository, query_id),
        DebouncedRequest::LoadMergeRefs { query_id } => collect_merge_refs(repository, query_id),
        DebouncedRequest::LoadStashes { query_id } => collect_stashes(repository, query_id),
        DebouncedRequest::LoadRemotes { query_id } => collect_remotes(repository, query_id),
        DebouncedRequest::Execute {
            id,
            action,
            cancellation,
        } => {
            return Some(execute_command(
                repository,
                id,
                &action,
                &cancellation,
                generation,
                prompts,
                events,
            ));
        }
        DebouncedRequest::Shutdown => return None,
    };
    events.send(query).ok()?;
    Some(collect_refresh(repository, generation))
}

fn debounce(requests: &Receiver<WorkerRequest>, refresh_pending: &AtomicBool) -> DebouncedRequest {
    loop {
        match requests.recv_timeout(DEBOUNCE) {
            Ok(WorkerRequest::RefreshRequested) => {
                refresh_pending.store(false, Ordering::Release);
            }
            Ok(WorkerRequest::Execute {
                id,
                action,
                cancellation,
            }) => {
                return DebouncedRequest::Execute {
                    id,
                    action,
                    cancellation,
                };
            }
            Ok(WorkerRequest::LoadHistory { query_id }) => {
                return DebouncedRequest::LoadHistory { query_id };
            }
            Ok(WorkerRequest::LoadCommitPatch {
                query_id,
                commit_id,
            }) => {
                return DebouncedRequest::LoadCommitPatch {
                    query_id,
                    commit_id,
                };
            }
            Ok(WorkerRequest::LoadBranches { query_id }) => {
                return DebouncedRequest::LoadBranches { query_id };
            }
            Ok(WorkerRequest::LoadMergeRefs { query_id }) => {
                return DebouncedRequest::LoadMergeRefs { query_id };
            }
            Ok(WorkerRequest::LoadStashes { query_id }) => {
                return DebouncedRequest::LoadStashes { query_id };
            }
            Ok(WorkerRequest::LoadRemotes { query_id }) => {
                return DebouncedRequest::LoadRemotes { query_id };
            }
            Ok(WorkerRequest::WatchFailed(_)) => {}
            Ok(WorkerRequest::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                return DebouncedRequest::Shutdown;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => return DebouncedRequest::Refresh,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use diffo_core::RepositorySource;

    use super::*;

    struct FakeRepository {
        collections: AtomicUsize,
    }

    struct DelayedRepository {
        starts: AtomicUsize,
    }

    impl RepositorySource for FakeRepository {
        fn snapshot(&self) -> Result<RepositorySnapshot> {
            self.collections.fetch_add(1, Ordering::Relaxed);
            Ok(RepositorySnapshot::default())
        }
    }

    impl Repository for FakeRepository {
        fn checkout_history(&self) -> Result<diffo_core::CheckoutHistory> {
            Ok(diffo_core::CheckoutHistory {
                head_commit: Some("abc".to_owned()),
                commits: vec![diffo_core::Commit {
                    id: "abc".to_owned(),
                    summary: "history".to_owned(),
                }],
            })
        }

        fn commit_patch(&self, commit_id: &str) -> Result<String> {
            Ok(format!("patch for {commit_id}"))
        }

        fn branches(&self) -> Result<Vec<diffo_core::BranchRef>> {
            Ok(Vec::new())
        }

        fn apply(
            &self,
            _action: &RepositoryAction,
        ) -> std::result::Result<OperationResult, OperationFailure> {
            Ok(OperationResult::Stage)
        }
    }

    impl RepositorySource for DelayedRepository {
        fn snapshot(&self) -> Result<RepositorySnapshot> {
            Ok(RepositorySnapshot::default())
        }
    }

    impl Repository for DelayedRepository {
        fn apply(
            &self,
            _action: &RepositoryAction,
        ) -> std::result::Result<OperationResult, OperationFailure> {
            Ok(OperationResult::Fetch { updated_refs: 0 })
        }

        fn apply_with_context(
            &self,
            action: &RepositoryAction,
            context: &RepositoryOperationContext,
        ) -> std::result::Result<OperationOutcome, OperationFailure> {
            self.starts.fetch_add(1, Ordering::Release);
            if *action == RepositoryAction::Fetch {
                while !context.cancellation.is_cancelled() {
                    thread::sleep(Duration::from_millis(1));
                }
                Ok(OperationOutcome::Cancelled)
            } else {
                Ok(OperationOutcome::Completed(OperationResult::Fetch {
                    updated_refs: 0,
                }))
            }
        }
    }

    #[test]
    fn event_burst_collects_once() {
        let repository = Arc::new(FakeRepository {
            collections: AtomicUsize::new(0),
        });
        let (requests, request_rx) = mpsc::channel();
        let (events, event_rx) = mpsc::channel();
        let prompts = Arc::new(PromptBroker::new(events.clone()));
        let pending = Arc::new(AtomicBool::new(false));
        let busy = Arc::new(AtomicBool::new(false));
        let worker_repository = Arc::clone(&repository) as Arc<dyn Repository>;
        let worker = thread::spawn({
            let pending = Arc::clone(&pending);
            let busy = Arc::clone(&busy);
            let prompts = Arc::clone(&prompts);
            move || {
                worker_loop(
                    &*worker_repository,
                    &request_rx,
                    &events,
                    &pending,
                    &AtomicBool::new(false),
                    &busy,
                    &prompts,
                );
            }
        });

        requests.send(WorkerRequest::RefreshRequested).unwrap();
        requests.send(WorkerRequest::RefreshRequested).unwrap();
        requests.send(WorkerRequest::RefreshRequested).unwrap();
        let event = event_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(matches!(
            event,
            RepositoryEvent::Update(RepositoryUpdate {
                generation: 1,
                kind: RepositoryUpdateKind::Snapshot(_),
            })
        ));
        assert_eq!(repository.collections.load(Ordering::Relaxed), 1);
        requests.send(WorkerRequest::Shutdown).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn worktree_invalidation_is_emitted_independently_of_the_snapshot() {
        let repository: Arc<dyn Repository> = Arc::new(FakeRepository {
            collections: AtomicUsize::new(0),
        });
        let (requests, request_rx) = mpsc::channel();
        let (events, event_rx) = mpsc::channel();
        let prompts = Arc::new(PromptBroker::new(events.clone()));
        let worker = thread::spawn(move || {
            worker_loop(
                &*repository,
                &request_rx,
                &events,
                &AtomicBool::new(false),
                &AtomicBool::new(true),
                &AtomicBool::new(false),
                &prompts,
            );
        });

        requests.send(WorkerRequest::RefreshRequested).unwrap();
        assert!(matches!(
            event_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            RepositoryEvent::WorktreeChanged
        ));
        assert!(matches!(
            event_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            RepositoryEvent::Update(RepositoryUpdate {
                generation: 1,
                kind: RepositoryUpdateKind::Snapshot(_),
            })
        ));
        requests.send(WorkerRequest::Shutdown).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn discovery_during_refresh_debounce_preserves_both_ordered_results() {
        let repository = Arc::new(FakeRepository {
            collections: AtomicUsize::new(0),
        });
        let (requests, request_rx) = mpsc::channel();
        let (events, event_rx) = mpsc::channel();
        let prompts = Arc::new(PromptBroker::new(events.clone()));
        let worker_repository = Arc::clone(&repository) as Arc<dyn Repository>;
        let worker = thread::spawn(move || {
            worker_loop(
                &*worker_repository,
                &request_rx,
                &events,
                &AtomicBool::new(false),
                &AtomicBool::new(false),
                &AtomicBool::new(false),
                &prompts,
            );
        });

        requests.send(WorkerRequest::RefreshRequested).unwrap();
        requests
            .send(WorkerRequest::LoadBranches {
                query_id: RepositoryQueryId(7),
            })
            .unwrap();

        assert!(matches!(
            event_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            RepositoryEvent::BranchesLoaded {
                query_id: RepositoryQueryId(7),
                ..
            }
        ));
        assert!(matches!(
            event_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            RepositoryEvent::Update(RepositoryUpdate {
                generation: 1,
                kind: RepositoryUpdateKind::Snapshot(_),
            })
        ));
        requests.send(WorkerRequest::Shutdown).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn history_and_commit_patch_queries_keep_their_identities() {
        let repository = FakeRepository {
            collections: AtomicUsize::new(0),
        };

        assert!(matches!(
            collect_history(&repository, RepositoryQueryId(7)),
            RepositoryEvent::HistoryLoaded {
                query_id: RepositoryQueryId(7),
                history: diffo_core::CheckoutHistory {
                    head_commit: Some(head),
                    commits,
                },
            } if head == "abc" && commits[0].summary == "history"
        ));
        assert!(matches!(
            collect_commit_patch(&repository, RepositoryQueryId(8), "abc".to_owned()),
            RepositoryEvent::CommitPatchLoaded {
                query_id: RepositoryQueryId(8),
                commit_id,
                patch,
            } if commit_id == "abc" && patch == "patch for abc"
        ));
    }

    #[test]
    fn shutdown_during_debounce_joins_worker() {
        let repository: Arc<dyn Repository> = Arc::new(FakeRepository {
            collections: AtomicUsize::new(0),
        });
        let (requests, request_rx) = mpsc::channel();
        let (events, _event_rx) = mpsc::channel();
        let prompts = Arc::new(PromptBroker::new(events.clone()));
        let worker = thread::spawn(move || {
            let pending = AtomicBool::new(false);
            let busy = AtomicBool::new(false);
            worker_loop(
                &*repository,
                &request_rx,
                &events,
                &pending,
                &AtomicBool::new(false),
                &busy,
                &prompts,
            );
        });

        requests.send(WorkerRequest::RefreshRequested).unwrap();
        requests.send(WorkerRequest::Shutdown).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn action_completion_is_distinct_from_watcher_snapshot() {
        let repository = FakeRepository {
            collections: AtomicUsize::new(0),
        };
        let mut generation = 0;
        let (events, _event_rx) = mpsc::channel();
        let prompts = Arc::new(PromptBroker::new(events.clone()));
        assert!(prompts.begin_operation(ApplicationCommandId(1), CancellationHandle::default()));

        let watcher = collect_refresh(&repository, &mut generation);
        let command = execute_command(
            &repository,
            ApplicationCommandId(1),
            &RepositoryAction::StageAll,
            &CancellationHandle::default(),
            &mut generation,
            &prompts,
            &events,
        );

        assert!(matches!(
            watcher,
            RepositoryEvent::Update(RepositoryUpdate {
                kind: RepositoryUpdateKind::Snapshot(_),
                ..
            })
        ));
        assert!(matches!(
            command,
            RepositoryEvent::Update(RepositoryUpdate {
                kind: RepositoryUpdateKind::CommandCompleted {
                    result: OperationResult::Stage,
                    ..
                },
                ..
            })
        ));
    }

    #[test]
    fn next_action_starts_only_after_cancellation_is_acknowledged() {
        let repository = Arc::new(DelayedRepository {
            starts: AtomicUsize::new(0),
        });
        let (requests, request_rx) = mpsc::channel();
        let (events, event_rx) = mpsc::channel();
        let prompts = Arc::new(PromptBroker::new(events.clone()));
        let worker_repository = Arc::clone(&repository) as Arc<dyn Repository>;
        assert!(prompts.begin_operation(ApplicationCommandId(1), CancellationHandle::default()));
        let worker = thread::spawn(move || {
            worker_loop(
                &*worker_repository,
                &request_rx,
                &events,
                &AtomicBool::new(false),
                &AtomicBool::new(false),
                &AtomicBool::new(false),
                &prompts,
            );
        });
        let cancellation = CancellationHandle::default();
        requests
            .send(WorkerRequest::Execute {
                id: ApplicationCommandId(1),
                action: RepositoryAction::Fetch,
                cancellation: cancellation.clone(),
            })
            .unwrap();
        requests
            .send(WorkerRequest::Execute {
                id: ApplicationCommandId(2),
                action: RepositoryAction::Sync,
                cancellation: CancellationHandle::default(),
            })
            .unwrap();

        while repository.starts.load(Ordering::Acquire) == 0 {
            thread::yield_now();
        }
        assert_eq!(repository.starts.load(Ordering::Acquire), 1);
        assert!(event_rx.try_recv().is_err());
        cancellation.cancel();

        assert!(matches!(
            event_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            RepositoryEvent::Update(RepositoryUpdate {
                kind: RepositoryUpdateKind::CommandCancelled {
                    command_id: ApplicationCommandId(1),
                    ..
                },
                ..
            })
        ));
        assert!(matches!(
            event_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            RepositoryEvent::Update(RepositoryUpdate {
                kind: RepositoryUpdateKind::CommandCompleted {
                    command_id: ApplicationCommandId(2),
                    ..
                },
                ..
            })
        ));
        requests.send(WorkerRequest::Shutdown).unwrap();
        worker.join().unwrap();
    }
}
