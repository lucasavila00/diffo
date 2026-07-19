use std::{process::Command, thread, time::Duration};

use diffo_core::CancellationHandle;
use nix::{errno::Errno, sys::signal::kill, unistd::Pid};

use super::operation::{CommandOutcome, run_cancellable};

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
    assert_eq!(kill(Pid::from_raw(child_pid), None), Err(Errno::ESRCH));
}
