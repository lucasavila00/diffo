use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread::{self, JoinHandle},
};

use anyhow::{Context, Result};
use diffo_core::{
    OperationFailure, OperationResult, Repository, RepositoryAction, RepositorySnapshot,
};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::worker::{Command, worker_loop};

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
        action: RepositoryAction,
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
