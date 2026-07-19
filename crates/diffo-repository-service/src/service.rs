use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, SyncSender, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{Context, Result};
use diffo_core::{
    ApplicationCommandId, BranchRef, CancellationHandle, GitPrompt, OperationFailure,
    OperationResult, PromptAnswer, PromptHandler, PromptId, Repository, RepositoryAction,
    RepositoryQueryId, RepositorySnapshot,
};

use crate::{
    watcher::RepositoryWatcher,
    worker::{WorkerRequest, worker_loop},
};

#[derive(Debug)]
pub enum RepositoryEvent {
    BranchesLoaded {
        query_id: RepositoryQueryId,
        branches: Vec<BranchRef>,
    },
    BranchesLoadFailed {
        query_id: RepositoryQueryId,
        message: String,
    },
    Prompt {
        command_id: ApplicationCommandId,
        prompt_id: PromptId,
        prompt: GitPrompt,
    },
    SnapshotRefreshed {
        generation: u64,
        snapshot: RepositorySnapshot,
    },
    RefreshFailed {
        generation: u64,
        message: String,
    },
    CommandCompleted {
        generation: u64,
        command_id: ApplicationCommandId,
        action: RepositoryAction,
        result: OperationResult,
        snapshot: RepositorySnapshot,
    },
    CommandFailed {
        generation: u64,
        command_id: ApplicationCommandId,
        failure: OperationFailure,
    },
    CommandCancelled {
        generation: u64,
        command_id: ApplicationCommandId,
        action: RepositoryAction,
    },
}

pub struct RepositoryService {
    requests: Sender<WorkerRequest>,
    events: Receiver<RepositoryEvent>,
    busy: Arc<AtomicBool>,
    prompts: Arc<PromptBroker>,
    watcher: Option<RepositoryWatcher>,
    worker: Option<JoinHandle<()>>,
}

struct ActivePrompt {
    id: PromptId,
    answer: SyncSender<PromptAnswer>,
}

struct PromptOperation {
    command_id: ApplicationCommandId,
    cancellation: CancellationHandle,
    next_prompt_id: u64,
    prompt: Option<ActivePrompt>,
}

#[derive(Default)]
struct PromptState {
    operation: Option<PromptOperation>,
}

pub(super) struct PromptBroker {
    events: Sender<RepositoryEvent>,
    state: Mutex<PromptState>,
}

impl PromptBroker {
    pub(super) fn new(events: Sender<RepositoryEvent>) -> Self {
        Self {
            events,
            state: Mutex::new(PromptState::default()),
        }
    }

    pub(super) fn begin_operation(
        &self,
        command_id: ApplicationCommandId,
        cancellation: CancellationHandle,
    ) -> bool {
        self.state.lock().is_ok_and(|mut state| {
            if state.operation.is_some() {
                return false;
            }
            state.operation = Some(PromptOperation {
                command_id,
                cancellation,
                next_prompt_id: 1,
                prompt: None,
            });
            true
        })
    }

    pub(super) fn finish_operation(&self, command_id: ApplicationCommandId) {
        if let Ok(mut state) = self.state.lock()
            && state
                .operation
                .as_ref()
                .is_some_and(|operation| operation.command_id == command_id)
        {
            state.operation = None;
        }
    }

    fn answer(
        &self,
        command_id: ApplicationCommandId,
        prompt_id: PromptId,
        answer: PromptAnswer,
    ) -> bool {
        let response = self.state.lock().ok().and_then(|mut state| {
            let operation = state.operation.as_mut()?;
            if operation.command_id != command_id
                || operation.prompt.as_ref().map(|prompt| prompt.id) != Some(prompt_id)
            {
                return None;
            }
            if matches!(answer, PromptAnswer::Cancel) {
                operation.cancellation.cancel();
            }
            let active = operation.prompt.take()?;
            operation.next_prompt_id = operation.next_prompt_id.saturating_add(1);
            Some((active.answer, answer))
        });
        response.is_some_and(|(sender, answer)| sender.send(answer).is_ok())
    }

    fn cancel_command(&self, command_id: ApplicationCommandId) -> bool {
        let active = self.state.lock().ok().and_then(|mut state| {
            let operation = state.operation.as_mut()?;
            if operation.command_id != command_id {
                return None;
            }
            operation.cancellation.cancel();
            Some(operation.prompt.take().map(|prompt| prompt.answer))
        });
        let found = active.is_some();
        if let Some(Some(sender)) = active {
            let _ = sender.send(PromptAnswer::Cancel);
        }
        found
    }

    fn cancel_active(&self) {
        let active = self.state.lock().ok().and_then(|mut state| {
            let operation = state.operation.as_mut()?;
            operation.cancellation.cancel();
            operation.prompt.take().map(|prompt| prompt.answer)
        });
        if let Some(sender) = active {
            let _ = sender.send(PromptAnswer::Cancel);
        }
    }

    fn discard(&self, command_id: ApplicationCommandId, prompt_id: PromptId) {
        if let Ok(mut state) = self.state.lock()
            && let Some(operation) = state.operation.as_mut()
            && operation.command_id == command_id
            && operation.prompt.as_ref().map(|prompt| prompt.id) == Some(prompt_id)
        {
            operation.prompt = None;
        }
    }
}

impl PromptHandler for PromptBroker {
    fn prompt(
        &self,
        id: PromptId,
        prompt: GitPrompt,
        cancellation: &CancellationHandle,
    ) -> PromptAnswer {
        let (answer_tx, answer_rx) = mpsc::sync_channel(1);
        let command_id = self.state.lock().ok().and_then(|mut state| {
            let operation = state.operation.as_mut()?;
            if operation.prompt.is_some()
                || id.0 != operation.next_prompt_id
                || operation.cancellation.is_cancelled()
            {
                return None;
            }
            operation.prompt = Some(ActivePrompt {
                id,
                answer: answer_tx,
            });
            Some(operation.command_id)
        });
        let Some(command_id) = command_id else {
            return PromptAnswer::Cancel;
        };
        if self
            .events
            .send(RepositoryEvent::Prompt {
                command_id,
                prompt_id: id,
                prompt,
            })
            .is_err()
        {
            self.discard(command_id, id);
            return PromptAnswer::Cancel;
        }
        loop {
            if cancellation.is_cancelled() {
                self.discard(command_id, id);
                return PromptAnswer::Cancel;
            }
            match answer_rx.recv_timeout(Duration::from_millis(20)) {
                Ok(answer) => return answer,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.discard(command_id, id);
                    return PromptAnswer::Cancel;
                }
            }
        }
    }
}

impl RepositoryService {
    /// Start the serialized repository worker and optionally watch repository paths.
    ///
    /// # Errors
    ///
    /// Returns an error when the optional watcher or worker cannot be started.
    pub fn start(repository: Arc<dyn Repository>, paths: Option<&[PathBuf]>) -> Result<Self> {
        let (requests, request_rx) = mpsc::channel();
        let (event_tx, events) = mpsc::channel();
        let prompts = Arc::new(PromptBroker::new(event_tx.clone()));
        let refresh_pending = Arc::new(AtomicBool::new(false));
        let watcher = paths
            .map(|paths| {
                RepositoryWatcher::start(paths, requests.clone(), Arc::clone(&refresh_pending))
            })
            .transpose()?;

        let busy = Arc::new(AtomicBool::new(false));
        let worker_busy = Arc::clone(&busy);
        let worker_prompts = Arc::clone(&prompts);
        let worker = thread::Builder::new()
            .name("diffo-repository-service".to_owned())
            .spawn(move || {
                worker_loop(
                    &*repository,
                    &request_rx,
                    &event_tx,
                    &refresh_pending,
                    &worker_busy,
                    &worker_prompts,
                );
            })
            .context("failed to start repository service worker")?;

        Ok(Self {
            requests,
            events,
            busy,
            prompts,
            watcher,
            worker: Some(worker),
        })
    }

    #[must_use]
    pub fn execute(
        &self,
        id: ApplicationCommandId,
        action: RepositoryAction,
        cancellation: CancellationHandle,
    ) -> bool {
        if !self.prompts.begin_operation(id, cancellation.clone()) {
            return false;
        }
        if self
            .requests
            .send(WorkerRequest::Execute {
                id,
                action,
                cancellation,
            })
            .is_err()
        {
            self.prompts.cancel_command(id);
            self.prompts.finish_operation(id);
            return false;
        }
        true
    }

    #[must_use]
    pub fn load_branches(&self, query_id: RepositoryQueryId) -> bool {
        self.requests
            .send(WorkerRequest::LoadBranches { query_id })
            .is_ok()
    }

    #[must_use]
    pub fn answer_prompt(
        &self,
        command_id: ApplicationCommandId,
        prompt_id: PromptId,
        answer: PromptAnswer,
    ) -> bool {
        self.prompts.answer(command_id, prompt_id, answer)
    }

    #[must_use]
    pub fn cancel_command(&self, command_id: ApplicationCommandId) -> bool {
        self.prompts.cancel_command(command_id)
    }

    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::Acquire)
    }

    /// Read one completed repository event without blocking.
    ///
    /// # Errors
    ///
    /// Returns `Disconnected` when the repository worker has stopped.
    pub fn try_recv(&self) -> Result<Option<RepositoryEvent>, TryRecvError> {
        match self.events.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(error @ TryRecvError::Disconnected) => Err(error),
        }
    }
}

impl Drop for RepositoryService {
    fn drop(&mut self) {
        self.watcher.take();
        self.prompts.cancel_active();
        let _ = self.requests.send(WorkerRequest::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests;
