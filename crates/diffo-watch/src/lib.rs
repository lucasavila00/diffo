use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{Context, Result};
use diffo_core::{OperationFailure, OperationResult, Repository, RepositoryAction, RepositorySnapshot};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

const DEBOUNCE: Duration = Duration::from_millis(100);

enum Command {
    Wake,
    Action(RepositoryAction),
    WatchError(String),
    Shutdown,
}

enum Debounced {
    Refresh,
    Action(RepositoryAction),
    Shutdown,
}

#[derive(Debug)]
pub enum RefreshResult {
    Snapshot {
        generation: u64,
        snapshot: RepositorySnapshot,
    },
    Error {
        generation: u64,
        message: String,
    },
    ActionCompleted {
        generation: u64,
        result: OperationResult,
        snapshot: RepositorySnapshot,
    },
    ActionFailed {
        generation: u64,
        failure: OperationFailure,
    },
}

pub struct RefreshService {
    commands: Sender<Command>,
    results: Receiver<RefreshResult>,
    busy: Arc<AtomicBool>,
    watcher: Option<RecommendedWatcher>,
    worker: Option<JoinHandle<()>>,
}

impl RefreshService {
    /// Start watching repository paths and collecting snapshots.
    ///
    /// # Errors
    ///
    /// Returns an error when the watcher or worker cannot be started.
    pub fn start(repository: Arc<dyn Repository>, paths: &[PathBuf]) -> Result<Self> {
        let (commands, command_rx) = mpsc::channel();
        let (result_tx, results) = mpsc::channel();
        let wake_pending = Arc::new(AtomicBool::new(false));
        let callback_pending = Arc::clone(&wake_pending);
        let callback_commands = commands.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
                Ok(_) => {
                    if !callback_pending.swap(true, Ordering::AcqRel) {
                        let _ = callback_commands.send(Command::Wake);
                    }
                }
                Err(error) => {
                    let _ = callback_commands.send(Command::WatchError(error.to_string()));
                }
            })
            .context("failed to create repository watcher")?;
        for path in paths {
            watcher
                .watch(path, RecursiveMode::Recursive)
                .with_context(|| format!("failed to watch {}", path.display()))?;
        }

        let busy = Arc::new(AtomicBool::new(false));
        let worker_busy = Arc::clone(&busy);
        let worker = thread::Builder::new()
            .name("diffo-repository-refresh".to_owned())
            .spawn(move || {
                worker_loop(
                    &*repository,
                    &command_rx,
                    &result_tx,
                    &wake_pending,
                    &worker_busy,
                );
            })
            .context("failed to start repository refresh worker")?;

        Ok(Self {
            commands,
            results,
            busy,
            watcher: Some(watcher),
            worker: Some(worker),
        })
    }

    pub fn apply(&self, action: RepositoryAction) {
        let _ = self.commands.send(Command::Action(action));
    }

    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::Acquire)
    }

    /// Read one completed refresh without blocking.
    ///
    /// # Errors
    ///
    /// Returns `Disconnected` when the refresh worker has stopped.
    pub fn try_recv(&self) -> Result<Option<RefreshResult>, TryRecvError> {
        match self.results.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(error @ TryRecvError::Disconnected) => Err(error),
        }
    }
}

impl Drop for RefreshService {
    fn drop(&mut self) {
        self.watcher.take();
        let _ = self.commands.send(Command::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn worker_loop(
    repository: &dyn Repository,
    commands: &Receiver<Command>,
    results: &Sender<RefreshResult>,
    wake_pending: &AtomicBool,
    busy: &AtomicBool,
) {
    let mut generation = 0_u64;
    while let Ok(command) = commands.recv() {
        busy.store(true, Ordering::Release);
        let outcome = match command {
            Command::Wake => {
                wake_pending.store(false, Ordering::Release);
                match debounce(commands, wake_pending) {
                    Debounced::Refresh => Some(collect(repository, None, &mut generation)),
                    Debounced::Action(action) => {
                        Some(collect(repository, Some(&action), &mut generation))
                    }
                    Debounced::Shutdown => break,
                }
            }
            Command::Action(action) => Some(collect(repository, Some(&action), &mut generation)),
            Command::WatchError(message) => {
                generation = generation.saturating_add(1);
                Some(RefreshResult::Error {
                    generation,
                    message: format!("repository watch failed: {message}"),
                })
            }
            Command::Shutdown => break,
        };
        if let Some(outcome) = outcome
            && results.send(outcome).is_err()
        {
            break;
        }
        busy.store(false, Ordering::Release);
    }
    busy.store(false, Ordering::Release);
}

fn debounce(commands: &Receiver<Command>, wake_pending: &AtomicBool) -> Debounced {
    loop {
        match commands.recv_timeout(DEBOUNCE) {
            Ok(Command::Wake) => wake_pending.store(false, Ordering::Release),
            Ok(Command::Action(action)) => return Debounced::Action(action),
            Ok(Command::WatchError(_)) => {}
            Ok(Command::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Debounced::Shutdown;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => return Debounced::Refresh,
        }
    }
}

fn collect(
    repository: &dyn Repository,
    action: Option<&RepositoryAction>,
    generation: &mut u64,
) -> RefreshResult {
    *generation = generation.saturating_add(1);
    match action {
        Some(action) => match repository.apply(action) {
            Ok(result) => match repository.snapshot() {
                Ok(snapshot) => RefreshResult::ActionCompleted {
                    generation: *generation,
                    result,
                    snapshot,
                },
                Err(error) => RefreshResult::ActionFailed {
                    generation: *generation,
                    failure: OperationFailure {
                        action: action.clone(),
                        kind: diffo_core::FailureKind::Unknown,
                        detail: error.to_string(),
                    },
                },
            },
            Err(failure) => RefreshResult::ActionFailed {
                generation: *generation,
                failure,
            },
        },
        None => match repository.snapshot() {
            Ok(snapshot) => RefreshResult::Snapshot {
                generation: *generation,
                snapshot,
            },
            Err(error) => RefreshResult::Error {
                generation: *generation,
                message: error.to_string(),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use diffo_core::{AccessMode, RepositorySource};

    use super::*;

    struct FakeRepository {
        collections: AtomicUsize,
    }

    impl RepositorySource for FakeRepository {
        fn snapshot(&self) -> Result<RepositorySnapshot> {
            self.collections.fetch_add(1, Ordering::Relaxed);
            Ok(RepositorySnapshot::default())
        }
    }

    impl Repository for FakeRepository {
        fn access_mode(&self) -> AccessMode {
            AccessMode::ReadWrite
        }

        fn apply(
            &self,
            _action: &RepositoryAction,
        ) -> std::result::Result<OperationResult, OperationFailure> {
            Ok(OperationResult::Stage)
        }
    }

    #[test]
    fn event_burst_collects_once() {
        let repository = Arc::new(FakeRepository {
            collections: AtomicUsize::new(0),
        });
        let (commands, command_rx) = mpsc::channel();
        let (results, result_rx) = mpsc::channel();
        let pending = Arc::new(AtomicBool::new(false));
        let busy = Arc::new(AtomicBool::new(false));
        let worker_repository = Arc::clone(&repository) as Arc<dyn Repository>;
        let worker = thread::spawn({
            let pending = Arc::clone(&pending);
            let busy = Arc::clone(&busy);
            move || {
                worker_loop(&*worker_repository, &command_rx, &results, &pending, &busy);
            }
        });

        commands.send(Command::Wake).unwrap();
        commands.send(Command::Wake).unwrap();
        commands.send(Command::Wake).unwrap();
        let result = result_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(matches!(
            result,
            RefreshResult::Snapshot { generation: 1, .. }
        ));
        assert_eq!(repository.collections.load(Ordering::Relaxed), 1);
        commands.send(Command::Shutdown).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn shutdown_during_debounce_joins_worker() {
        let repository: Arc<dyn Repository> = Arc::new(FakeRepository {
            collections: AtomicUsize::new(0),
        });
        let (commands, command_rx) = mpsc::channel();
        let (results, _result_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let pending = AtomicBool::new(false);
            let busy = AtomicBool::new(false);
            worker_loop(&*repository, &command_rx, &results, &pending, &busy);
        });

        commands.send(Command::Wake).unwrap();
        commands.send(Command::Shutdown).unwrap();
        worker.join().unwrap();
    }

    #[test]
    fn action_completion_is_distinct_from_watcher_snapshot() {
        let repository = FakeRepository {
            collections: AtomicUsize::new(0),
        };
        let mut generation = 0;

        let watcher = collect(&repository, None, &mut generation);
        let action = collect(
            &repository,
            Some(&RepositoryAction::StageAll),
            &mut generation,
        );

        assert!(matches!(watcher, RefreshResult::Snapshot { .. }));
        assert!(matches!(
            action,
            RefreshResult::ActionCompleted {
                result: OperationResult::Stage,
                ..
            }
        ));
    }
}
