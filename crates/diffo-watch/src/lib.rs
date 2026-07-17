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
use diffo_core::{Repository, RepositoryAction, RepositorySnapshot};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

const DEBOUNCE: Duration = Duration::from_millis(100);

enum Command {
    Wake,
    Action(RepositoryAction),
    WatchError(String),
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
                worker_loop(repository, command_rx, result_tx, wake_pending, worker_busy);
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
    repository: Arc<dyn Repository>,
    commands: Receiver<Command>,
    results: Sender<RefreshResult>,
    wake_pending: Arc<AtomicBool>,
    busy: Arc<AtomicBool>,
) {
    let mut generation = 0_u64;
    while let Ok(command) = commands.recv() {
        busy.store(true, Ordering::Release);
        let outcome = match command {
            Command::Wake => {
                wake_pending.store(false, Ordering::Release);
                debounce(&commands, &wake_pending)
                    .map(|action| collect(&*repository, action, &mut generation))
            }
            Command::Action(action) => Some(collect(&*repository, Some(action), &mut generation)),
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

fn debounce(
    commands: &Receiver<Command>,
    wake_pending: &AtomicBool,
) -> Option<Option<RepositoryAction>> {
    loop {
        match commands.recv_timeout(DEBOUNCE) {
            Ok(Command::Wake) => wake_pending.store(false, Ordering::Release),
            Ok(Command::Action(action)) => return Some(Some(action)),
            Ok(Command::WatchError(_)) => continue,
            Ok(Command::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            Err(mpsc::RecvTimeoutError::Timeout) => return Some(None),
        }
    }
}

fn collect(
    repository: &dyn Repository,
    action: Option<RepositoryAction>,
    generation: &mut u64,
) -> RefreshResult {
    *generation = generation.saturating_add(1);
    let result = action
        .as_ref()
        .map_or(Ok(()), |action| repository.apply(action))
        .and_then(|()| repository.snapshot());
    match result {
        Ok(snapshot) => RefreshResult::Snapshot {
            generation: *generation,
            snapshot,
        },
        Err(error) => RefreshResult::Error {
            generation: *generation,
            message: error.to_string(),
        },
    }
}
