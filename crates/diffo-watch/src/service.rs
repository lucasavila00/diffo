use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, SyncSender, TryRecvError},
    },
    thread::{self, JoinHandle},
};

use anyhow::{Context, Result};
use diffo_core::{
    GitPrompt, OperationFailure, OperationResult, PromptAnswer, PromptHandler, PromptId,
    Repository, RepositoryAction, RepositorySnapshot,
};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::worker::{Command, worker_loop};

#[derive(Debug)]
pub enum RefreshResult {
    Prompt {
        id: PromptId,
        prompt: GitPrompt,
    },
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
    prompts: Arc<PromptBroker>,
    watcher: Option<RecommendedWatcher>,
    worker: Option<JoinHandle<()>>,
}

struct PromptState {
    next_id: u64,
    active: Option<(PromptId, SyncSender<PromptAnswer>)>,
}

pub(super) struct PromptBroker {
    results: Sender<RefreshResult>,
    pub(super) cancelled: Arc<AtomicBool>,
    state: Mutex<PromptState>,
}

impl PromptBroker {
    pub(super) fn new(results: Sender<RefreshResult>) -> Self {
        Self {
            results,
            cancelled: Arc::new(AtomicBool::new(false)),
            state: Mutex::new(PromptState {
                next_id: 1,
                active: None,
            }),
        }
    }

    pub(super) fn begin_operation(&self) {
        self.cancelled.store(false, Ordering::Release);
        if let Ok(mut state) = self.state.lock() {
            state.next_id = 1;
            state.active = None;
        }
    }

    fn answer(&self, id: PromptId, answer: PromptAnswer) -> bool {
        let sender = self.state.lock().ok().and_then(|mut state| {
            let (active_id, _) = state.active.as_ref()?;
            if *active_id != id {
                return None;
            }
            let (_, sender) = state.active.take()?;
            state.next_id = state.next_id.saturating_add(1);
            Some(sender)
        });
        sender.is_some_and(|sender| sender.send(answer).is_ok())
    }

    fn discard(&self, id: PromptId) {
        if let Ok(mut state) = self.state.lock()
            && state
                .active
                .as_ref()
                .is_some_and(|(active, _)| *active == id)
        {
            state.active = None;
        }
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        let sender = self
            .state
            .lock()
            .ok()
            .and_then(|mut state| state.active.take().map(|(_, sender)| sender));
        if let Some(sender) = sender {
            let _ = sender.send(PromptAnswer::Cancel);
        }
    }
}

impl PromptHandler for PromptBroker {
    fn prompt(&self, id: PromptId, prompt: GitPrompt, cancelled: &AtomicBool) -> PromptAnswer {
        let (answer_tx, answer_rx) = mpsc::sync_channel(1);
        let accepted = self.state.lock().is_ok_and(|mut state| {
            if state.active.is_some() || id.0 != state.next_id {
                return false;
            }
            state.active = Some((id, answer_tx));
            true
        });
        if !accepted {
            return PromptAnswer::Cancel;
        }
        if self
            .results
            .send(RefreshResult::Prompt { id, prompt })
            .is_err()
        {
            self.discard(id);
            return PromptAnswer::Cancel;
        }
        loop {
            if cancelled.load(Ordering::Acquire) || self.cancelled.load(Ordering::Acquire) {
                self.discard(id);
                return PromptAnswer::Cancel;
            }
            match answer_rx.recv_timeout(std::time::Duration::from_millis(20)) {
                Ok(answer) => return answer,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.discard(id);
                    return PromptAnswer::Cancel;
                }
            }
        }
    }
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
        let prompts = Arc::new(PromptBroker::new(result_tx.clone()));
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
        let worker_prompts = Arc::clone(&prompts);
        let worker = thread::Builder::new()
            .name("diffo-repository-refresh".to_owned())
            .spawn(move || {
                worker_loop(
                    &*repository,
                    &command_rx,
                    &result_tx,
                    &wake_pending,
                    &worker_busy,
                    &worker_prompts,
                );
            })
            .context("failed to start repository refresh worker")?;

        Ok(Self {
            commands,
            results,
            busy,
            prompts,
            watcher: Some(watcher),
            worker: Some(worker),
        })
    }

    pub fn apply(&self, action: RepositoryAction) {
        let _ = self.commands.send(Command::Action(action));
    }

    #[must_use]
    pub fn answer_prompt(&self, id: PromptId, answer: PromptAnswer) -> bool {
        self.prompts.answer(id, answer)
    }

    pub fn cancel(&self) {
        self.prompts.cancel();
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

#[cfg(test)]
mod tests;

impl Drop for RefreshService {
    fn drop(&mut self) {
        self.watcher.take();
        self.prompts.cancel();
        let _ = self.commands.send(Command::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
