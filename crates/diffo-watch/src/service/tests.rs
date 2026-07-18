use std::thread;

use super::*;

fn username() -> GitPrompt {
    GitPrompt::Username {
        host: "example.com".to_owned(),
    }
}

#[test]
fn prompt_answers_bypass_the_worker_command_queue() {
    let (results, result_rx) = mpsc::channel();
    let broker = Arc::new(PromptBroker::new(results));
    let worker = {
        let broker = Arc::clone(&broker);
        thread::spawn(move || broker.prompt(PromptId(1), username(), &broker.cancelled))
    };

    assert!(matches!(
        result_rx.recv_timeout(std::time::Duration::from_secs(1)),
        Ok(RefreshResult::Prompt {
            id: PromptId(1),
            ..
        })
    ));
    assert!(broker.answer(PromptId(1), PromptAnswer::Text("answer".to_owned())));
    assert!(matches!(worker.join(), Ok(PromptAnswer::Text(answer)) if answer == "answer"));
}

#[test]
fn rejects_concurrent_duplicate_and_stale_prompt_ids() {
    let (results, result_rx) = mpsc::channel();
    let broker = Arc::new(PromptBroker::new(results));
    let waiting = {
        let broker = Arc::clone(&broker);
        thread::spawn(move || broker.prompt(PromptId(1), username(), &broker.cancelled))
    };
    let _ = result_rx.recv_timeout(std::time::Duration::from_secs(1));

    assert!(matches!(
        broker.prompt(PromptId(1), username(), &broker.cancelled),
        PromptAnswer::Cancel
    ));
    assert!(matches!(
        broker.prompt(PromptId(2), username(), &broker.cancelled),
        PromptAnswer::Cancel
    ));
    assert!(broker.answer(PromptId(1), PromptAnswer::Text("first".to_owned())));
    assert!(matches!(waiting.join(), Ok(PromptAnswer::Text(_))));
    assert!(matches!(
        broker.prompt(PromptId(1), username(), &broker.cancelled),
        PromptAnswer::Cancel
    ));
}

#[test]
fn accepts_sequential_prompts_and_rejects_unknown_answers() {
    let (results, result_rx) = mpsc::channel();
    let broker = Arc::new(PromptBroker::new(results));
    for id in [PromptId(1), PromptId(2)] {
        let waiting = {
            let broker = Arc::clone(&broker);
            thread::spawn(move || broker.prompt(id, username(), &broker.cancelled))
        };
        let _ = result_rx.recv_timeout(std::time::Duration::from_secs(1));
        assert!(!broker.answer(PromptId(99), PromptAnswer::Cancel));
        assert!(broker.answer(id, PromptAnswer::Text("answer".to_owned())));
        assert!(matches!(waiting.join(), Ok(PromptAnswer::Text(_))));
    }
}

#[test]
fn cancel_and_disconnect_return_no_answer() {
    let (results, result_rx) = mpsc::channel();
    let broker = Arc::new(PromptBroker::new(results));
    let waiting = {
        let broker = Arc::clone(&broker);
        thread::spawn(move || broker.prompt(PromptId(1), username(), &broker.cancelled))
    };
    let _ = result_rx.recv_timeout(std::time::Duration::from_secs(1));
    broker.cancel();
    assert!(matches!(waiting.join(), Ok(PromptAnswer::Cancel)));

    let (results, result_rx) = mpsc::channel();
    drop(result_rx);
    let broker = PromptBroker::new(results);
    assert!(matches!(
        broker.prompt(PromptId(1), username(), &broker.cancelled),
        PromptAnswer::Cancel
    ));
}
