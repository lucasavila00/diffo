use std::thread;

use super::*;
use diffo_core::{OperationOutcome, RepositoryOperationContext, RepositorySource};

fn username() -> GitPrompt {
    GitPrompt::Username {
        host: "example.com".to_owned(),
    }
}

fn broker() -> (
    Arc<PromptBroker>,
    mpsc::Receiver<RepositoryEvent>,
    CancellationHandle,
) {
    let (events, event_rx) = mpsc::channel();
    let broker = Arc::new(PromptBroker::new(events));
    let cancellation = CancellationHandle::default();
    assert!(broker.begin_operation(ApplicationCommandId(7), cancellation.clone()));
    (broker, event_rx, cancellation)
}

#[test]
fn prompt_answers_bypass_the_worker_request_lane() {
    let (broker, event_rx, cancellation) = broker();
    let waiting = {
        let broker = Arc::clone(&broker);
        let cancellation = cancellation.clone();
        thread::spawn(move || broker.prompt(PromptId(1), username(), &cancellation))
    };

    assert!(matches!(
        event_rx.recv_timeout(Duration::from_secs(1)),
        Ok(RepositoryEvent::Prompt {
            command_id: ApplicationCommandId(7),
            prompt_id: PromptId(1),
            ..
        })
    ));
    assert!(broker.answer(
        ApplicationCommandId(7),
        PromptId(1),
        PromptAnswer::Text("answer".to_owned())
    ));
    assert!(matches!(waiting.join(), Ok(PromptAnswer::Text(answer)) if answer == "answer"));
}

#[test]
fn rejects_wrong_command_concurrent_duplicate_and_stale_prompts() {
    let (broker, event_rx, cancellation) = broker();
    let waiting = {
        let broker = Arc::clone(&broker);
        let cancellation = cancellation.clone();
        thread::spawn(move || broker.prompt(PromptId(1), username(), &cancellation))
    };
    let _ = event_rx.recv_timeout(Duration::from_secs(1));

    assert!(!broker.answer(ApplicationCommandId(8), PromptId(1), PromptAnswer::Cancel));
    assert!(matches!(
        broker.prompt(PromptId(1), username(), &cancellation),
        PromptAnswer::Cancel
    ));
    assert!(matches!(
        broker.prompt(PromptId(2), username(), &cancellation),
        PromptAnswer::Cancel
    ));
    assert!(broker.answer(
        ApplicationCommandId(7),
        PromptId(1),
        PromptAnswer::Text("first".to_owned())
    ));
    assert!(matches!(waiting.join(), Ok(PromptAnswer::Text(_))));
    assert!(matches!(
        broker.prompt(PromptId(1), username(), &cancellation),
        PromptAnswer::Cancel
    ));
}

#[test]
fn accepts_sequential_prompts_for_one_command() {
    let (broker, event_rx, cancellation) = broker();
    for id in [PromptId(1), PromptId(2)] {
        let waiting = {
            let broker = Arc::clone(&broker);
            let cancellation = cancellation.clone();
            thread::spawn(move || broker.prompt(id, username(), &cancellation))
        };
        let _ = event_rx.recv_timeout(Duration::from_secs(1));
        assert!(broker.answer(
            ApplicationCommandId(7),
            id,
            PromptAnswer::Text("answer".to_owned())
        ));
        assert!(matches!(waiting.join(), Ok(PromptAnswer::Text(_))));
    }
}

#[test]
fn cancelling_a_prompt_cancels_the_whole_command() {
    let (broker, event_rx, cancellation) = broker();
    let waiting = {
        let broker = Arc::clone(&broker);
        let cancellation = cancellation.clone();
        thread::spawn(move || broker.prompt(PromptId(1), username(), &cancellation))
    };
    let _ = event_rx.recv_timeout(Duration::from_secs(1));

    assert!(broker.answer(ApplicationCommandId(7), PromptId(1), PromptAnswer::Cancel));
    assert!(cancellation.is_cancelled());
    assert!(matches!(waiting.join(), Ok(PromptAnswer::Cancel)));
}

struct PromptingRepository;

impl RepositorySource for PromptingRepository {
    fn snapshot(&self) -> Result<RepositorySnapshot> {
        Ok(RepositorySnapshot::default())
    }
}

impl Repository for PromptingRepository {
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
        if *action == RepositoryAction::Fetch {
            let answer = context
                .prompts
                .prompt(PromptId(1), username(), &context.cancellation);
            if matches!(answer, PromptAnswer::Cancel) || context.cancellation.is_cancelled() {
                return Ok(OperationOutcome::Cancelled);
            }
            return Ok(OperationOutcome::Completed(OperationResult::Fetch {
                updated_refs: 0,
            }));
        }
        Ok(OperationOutcome::Completed(OperationResult::Fetch {
            updated_refs: 0,
        }))
    }
}

#[test]
fn next_command_is_accepted_only_after_prompt_cancellation_is_acknowledged() {
    let repository: Arc<dyn Repository> = Arc::new(PromptingRepository);
    let service = RepositoryService::start(repository, None).unwrap();
    let first = ApplicationCommandId(1);
    let second = ApplicationCommandId(2);
    assert!(service.execute(
        first,
        RepositoryAction::Fetch,
        CancellationHandle::default(),
    ));
    assert!(matches!(
        service.events.recv_timeout(Duration::from_secs(1)),
        Ok(RepositoryEvent::Prompt {
            command_id: ApplicationCommandId(1),
            prompt_id: PromptId(1),
            ..
        })
    ));
    assert!(!service.execute(
        second,
        RepositoryAction::Sync,
        CancellationHandle::default(),
    ));
    assert!(service.answer_prompt(first, PromptId(1), PromptAnswer::Cancel));
    assert!(matches!(
        service.events.recv_timeout(Duration::from_secs(1)),
        Ok(RepositoryEvent::Update(RepositoryUpdate {
            kind: RepositoryUpdateKind::CommandCancelled {
                command_id: ApplicationCommandId(1),
                ..
            },
            ..
        }))
    ));

    assert!(service.execute(
        second,
        RepositoryAction::Sync,
        CancellationHandle::default(),
    ));
    assert!(matches!(
        service.events.recv_timeout(Duration::from_secs(1)),
        Ok(RepositoryEvent::Update(RepositoryUpdate {
            kind: RepositoryUpdateKind::CommandCompleted {
                command_id: ApplicationCommandId(2),
                ..
            },
            ..
        }))
    ));
}
