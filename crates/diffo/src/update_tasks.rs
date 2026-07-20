use std::{
    process::{Command, Stdio},
    sync::mpsc::{Receiver, Sender, channel},
    thread,
    time::Duration,
};

use diffo_app::workbench::{UpdateOutcome, Workbench};
use diffo_core::{ApplicationCommandId, CancellationHandle};

pub(crate) struct UpdateTasks {
    sender: Sender<(ApplicationCommandId, UpdateOutcome)>,
    receiver: Receiver<(ApplicationCommandId, UpdateOutcome)>,
}

impl UpdateTasks {
    pub(crate) fn new() -> Self {
        let (sender, receiver) = channel();
        Self { sender, receiver }
    }

    pub(crate) fn start_update(&self, id: ApplicationCommandId, cancellation: CancellationHandle) {
        let sender = self.sender.clone();
        thread::spawn(move || {
            let outcome = run_update_process(&cancellation);
            let _ = sender.send((id, outcome));
        });
    }

    pub(crate) fn drain(&self, workbench: &mut Workbench) {
        while let Ok((id, outcome)) = self.receiver.try_recv() {
            workbench.update_finished(id, outcome);
        }
    }
}

fn run_update_process(cancellation: &CancellationHandle) -> UpdateOutcome {
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => return UpdateOutcome::Failed(format!("Update failed: {error}")),
    };
    let mut child = match Command::new(executable)
        .arg("update")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return UpdateOutcome::Failed(format!("Update failed: {error}")),
    };
    loop {
        if cancellation.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return UpdateOutcome::Failed("Diffo update cancelled".to_owned());
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(error) => return UpdateOutcome::Failed(format!("Update failed: {error}")),
        }
    }
    match child.wait_with_output() {
        Ok(output) if output.status.success() => {
            UpdateOutcome::Succeeded(String::from_utf8_lossy(&output.stdout).trim().to_owned())
        }
        Ok(output) => {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            UpdateOutcome::Failed(if message.is_empty() {
                "Diffo update failed".to_owned()
            } else {
                message
            })
        }
        Err(error) => UpdateOutcome::Failed(format!("Update failed: {error}")),
    }
}
