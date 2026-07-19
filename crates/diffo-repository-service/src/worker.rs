use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::Duration,
};

use diffo_core::{
    ApplicationCommandId, CancellationHandle, OperationFailure, OperationOutcome, Repository,
    RepositoryAction, RepositoryOperationContext, RepositoryQueryId, RepositoryUpdate,
    RepositoryUpdateKind,
};

use crate::service::{PromptBroker, RepositoryEvent};

#[cfg(test)]
use anyhow::Result;
#[cfg(test)]
use diffo_core::{OperationResult, RepositorySnapshot};
#[cfg(test)]
use std::thread;

const DEBOUNCE: Duration = Duration::from_millis(100);

pub(super) enum WorkerRequest {
    RefreshRequested,
    LoadBranches {
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
    LoadBranches {
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
    busy: &AtomicBool,
    prompts: &Arc<PromptBroker>,
) {
    let mut generation = 0_u64;
    while let Ok(request) = requests.recv() {
        busy.store(true, Ordering::Release);
        let event = match request {
            WorkerRequest::RefreshRequested => {
                refresh_pending.store(false, Ordering::Release);
                match debounce(requests, refresh_pending) {
                    DebouncedRequest::Refresh => Some(collect_refresh(repository, &mut generation)),
                    DebouncedRequest::LoadBranches { query_id } => {
                        if events.send(collect_branches(repository, query_id)).is_err() {
                            break;
                        }
                        Some(collect_refresh(repository, &mut generation))
                    }
                    DebouncedRequest::Execute {
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
                    )),
                    DebouncedRequest::Shutdown => break,
                }
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
            )),
            WorkerRequest::LoadBranches { query_id } => {
                Some(collect_branches(repository, query_id))
            }
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
            Ok(WorkerRequest::LoadBranches { query_id }) => {
                return DebouncedRequest::LoadBranches { query_id };
            }
            Ok(WorkerRequest::WatchFailed(_)) => {}
            Ok(WorkerRequest::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                return DebouncedRequest::Shutdown;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => return DebouncedRequest::Refresh,
        }
    }
}

fn collect_branches(repository: &dyn Repository, query_id: RepositoryQueryId) -> RepositoryEvent {
    match repository.branches() {
        Ok(branches) => RepositoryEvent::BranchesLoaded { query_id, branches },
        Err(error) => RepositoryEvent::BranchesLoadFailed {
            query_id,
            message: error.to_string(),
        },
    }
}

fn collect_refresh(repository: &dyn Repository, generation: &mut u64) -> RepositoryEvent {
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

fn execute_command(
    repository: &dyn Repository,
    command_id: ApplicationCommandId,
    action: &RepositoryAction,
    cancellation: &CancellationHandle,
    generation: &mut u64,
    prompts: &Arc<PromptBroker>,
) -> RepositoryEvent {
    *generation = generation.saturating_add(1);
    let context = RepositoryOperationContext::new(
        Arc::clone(prompts) as Arc<dyn diffo_core::PromptHandler>,
        cancellation.clone(),
    );
    let event = match repository.apply_with_context(action, &context) {
        Ok(OperationOutcome::Completed(result)) => match repository.snapshot() {
            Ok(snapshot) => RepositoryEvent::Update(RepositoryUpdate {
                generation: *generation,
                kind: RepositoryUpdateKind::CommandCompleted {
                    command_id,
                    action: action.clone(),
                    result,
                    snapshot,
                },
            }),
            Err(error) => RepositoryEvent::Update(RepositoryUpdate {
                generation: *generation,
                kind: RepositoryUpdateKind::CommandFailed {
                    command_id,
                    failure: OperationFailure {
                        action: action.clone(),
                        kind: diffo_core::FailureKind::Unknown,
                        detail: error.to_string(),
                    },
                },
            }),
        },
        Ok(OperationOutcome::Cancelled) => RepositoryEvent::Update(RepositoryUpdate {
            generation: *generation,
            kind: RepositoryUpdateKind::CommandCancelled {
                command_id,
                action: action.clone(),
            },
        }),
        Err(failure) => RepositoryEvent::Update(RepositoryUpdate {
            generation: *generation,
            kind: RepositoryUpdateKind::CommandFailed {
                command_id,
                failure,
            },
        }),
    };
    prompts.finish_operation(command_id);
    event
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
                Ok(OperationOutcome::Completed(OperationResult::Pull {
                    commits: 0,
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
        let prompts = Arc::new(PromptBroker::new(events));
        assert!(prompts.begin_operation(ApplicationCommandId(1), CancellationHandle::default()));

        let watcher = collect_refresh(&repository, &mut generation);
        let command = execute_command(
            &repository,
            ApplicationCommandId(1),
            &RepositoryAction::StageAll,
            &CancellationHandle::default(),
            &mut generation,
            &prompts,
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
                action: RepositoryAction::Pull,
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
