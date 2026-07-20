use std::{
    process::Command,
    thread,
    time::{Duration, Instant},
};

use diffo_core::CancellationHandle;
use nix::{errno::Errno, sys::signal::kill, unistd::Pid};

use super::operation::{CommandOutcome, protected_push_destination, run_cancellable};

#[test]
fn protects_only_pushes_to_exact_main_and_master_destinations() {
    assert_eq!(
        protected_push_destination("origin", "refs/heads/main", 2).as_deref(),
        Some("origin/main")
    );
    assert_eq!(
        protected_push_destination("upstream", "refs/heads/master", 1).as_deref(),
        Some("upstream/master")
    );
    for branch in ["main-next", "masterpiece", "Main", "MASTER", "topic"] {
        assert_eq!(
            protected_push_destination("origin", &format!("refs/heads/{branch}"), 1),
            None
        );
    }
    assert_eq!(
        protected_push_destination("origin", "refs/heads/main", 0),
        None
    );
}

#[test]
fn cancellation_reaps_the_operation_process_group() {
    let directory = tempfile::tempdir().unwrap();
    let pid_path = directory.path().join("child.pid");
    let script = format!("sleep 30 & echo $! > {}; wait", pid_path.display());
    let cancellation = CancellationHandle::default();
    let trigger = {
        let cancellation = cancellation.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            cancellation.cancel();
        })
    };
    let mut command = Command::new("sh");
    command.args(["-c", &script]);

    assert!(matches!(
        run_cancellable(&mut command, &cancellation),
        Ok(CommandOutcome::Cancelled)
    ));
    trigger.join().unwrap();
    let child_pid = std::fs::read_to_string(pid_path)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && kill(Pid::from_raw(child_pid), None).is_ok() {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(kill(Pid::from_raw(child_pid), None), Err(Errno::ESRCH));
}
