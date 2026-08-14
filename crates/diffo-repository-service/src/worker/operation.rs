use std::sync::{Arc, mpsc::Sender};

use diffo_core::{
    ApplicationCommandId, CancellationHandle, OperationFailure, OperationOutcome, Repository,
    RepositoryAction, RepositoryOperationContext, RepositoryUpdate, RepositoryUpdateKind,
    SyncProgress,
};

use crate::service::{PromptBroker, RepositoryEvent};

struct CommandProgressReporter {
    command_id: ApplicationCommandId,
    events: Sender<RepositoryEvent>,
}

impl diffo_core::ProgressHandler for CommandProgressReporter {
    fn progress(&self, progress: SyncProgress) {
        let _ = self.events.send(RepositoryEvent::Progress {
            command_id: self.command_id,
            progress,
        });
    }
}

pub(super) fn execute(
    repository: &dyn Repository,
    command_id: ApplicationCommandId,
    action: &RepositoryAction,
    cancellation: &CancellationHandle,
    generation: &mut u64,
    prompts: &Arc<PromptBroker>,
    events: &Sender<RepositoryEvent>,
) -> RepositoryEvent {
    *generation = generation.saturating_add(1);
    let context = RepositoryOperationContext::with_progress(
        Arc::clone(prompts) as Arc<dyn diffo_core::PromptHandler>,
        cancellation.clone(),
        Arc::new(CommandProgressReporter {
            command_id,
            events: events.clone(),
        }),
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
            Err(error) => failed_snapshot(command_id, action, *generation, error.to_string()),
        },
        Ok(OperationOutcome::Cancelled) => match repository.snapshot() {
            Ok(snapshot) => RepositoryEvent::Update(RepositoryUpdate {
                generation: *generation,
                kind: RepositoryUpdateKind::CommandCancelled {
                    command_id,
                    action: action.clone(),
                    snapshot,
                },
            }),
            Err(error) => failed_snapshot(command_id, action, *generation, error.to_string()),
        },
        Err(failure) => RepositoryEvent::Update(RepositoryUpdate {
            generation: *generation,
            kind: RepositoryUpdateKind::CommandFailed {
                command_id,
                failure,
                snapshot: repository.snapshot().ok(),
            },
        }),
    };
    prompts.finish_operation(command_id);
    event
}

fn failed_snapshot(
    command_id: ApplicationCommandId,
    action: &RepositoryAction,
    generation: u64,
    detail: String,
) -> RepositoryEvent {
    RepositoryEvent::Update(RepositoryUpdate {
        generation,
        kind: RepositoryUpdateKind::CommandFailed {
            command_id,
            failure: OperationFailure {
                action: action.clone(),
                kind: diffo_core::FailureKind::Unknown,
                detail,
            },
            snapshot: None,
        },
    })
}
