use std::{
    io,
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const SLOW_START_DELAY: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StartupPhase {
    FindingGitRepository,
    LoadingMockRepository,
    ResolvingRepositoryPaths,
    ReadingRepositoryState,
    StartingRepositoryServices,
    PreparingInterface,
}

impl StartupPhase {
    const fn message(self) -> &'static str {
        match self {
            Self::FindingGitRepository => "finding the Git repository",
            Self::LoadingMockRepository => "loading the mock repository",
            Self::ResolvingRepositoryPaths => "resolving repository paths",
            Self::ReadingRepositoryState => "reading Git status and diffs",
            Self::StartingRepositoryServices => "starting repository services",
            Self::PreparingInterface => "preparing the interface",
        }
    }
}

pub(crate) struct StartupReporter {
    phases: Option<Sender<StartupPhase>>,
    worker: Option<JoinHandle<()>>,
}

impl StartupReporter {
    pub(crate) fn start(initial: StartupPhase) -> Self {
        let (phases, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("diffo-startup-reporter".to_owned())
            .spawn(move || report_slow_start(&receiver, initial))
            .ok();
        Self {
            phases: Some(phases),
            worker,
        }
    }

    pub(crate) fn phase(&self, phase: StartupPhase) {
        if let Some(phases) = &self.phases {
            let _ = phases.send(phase);
        }
    }

    pub(crate) fn finish(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        drop(self.phases.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for StartupReporter {
    fn drop(&mut self) {
        self.stop();
    }
}

fn report_slow_start(receiver: &Receiver<StartupPhase>, initial: StartupPhase) {
    let Some(mut phase) = wait_for_slow_start(receiver, initial, SLOW_START_DELAY) else {
        return;
    };
    let mut stderr = io::stderr();
    let _ = write_slow_start(&mut stderr, phase);

    while let Ok(next) = receiver.recv() {
        if next != phase {
            phase = next;
            let _ = write_phase(&mut stderr, phase);
        }
    }
}

fn wait_for_slow_start(
    receiver: &Receiver<StartupPhase>,
    initial: StartupPhase,
    delay: Duration,
) -> Option<StartupPhase> {
    let deadline = Instant::now() + delay;
    let mut phase = initial;
    loop {
        match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(next) => phase = next,
            Err(RecvTimeoutError::Timeout) => return Some(phase),
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}

fn write_slow_start(output: &mut impl io::Write, phase: StartupPhase) -> io::Result<()> {
    writeln!(output, "Diffo is still starting after 3 seconds.")?;
    write_phase(output, phase)
}

fn write_phase(output: &mut impl io::Write, phase: StartupPhase) -> io::Result<()> {
    writeln!(output, "[startup] {}", phase.message())?;
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_startup_stays_silent() {
        let (sender, receiver) = mpsc::channel();
        drop(sender);

        assert_eq!(
            wait_for_slow_start(
                &receiver,
                StartupPhase::FindingGitRepository,
                SLOW_START_DELAY
            ),
            None
        );
    }

    #[test]
    fn threshold_reports_the_latest_phase() {
        let (sender, receiver) = mpsc::channel();
        sender.send(StartupPhase::ResolvingRepositoryPaths).unwrap();
        sender.send(StartupPhase::ReadingRepositoryState).unwrap();

        assert_eq!(SLOW_START_DELAY, Duration::from_secs(3));
        assert_eq!(
            wait_for_slow_start(
                &receiver,
                StartupPhase::FindingGitRepository,
                Duration::ZERO
            ),
            Some(StartupPhase::ReadingRepositoryState)
        );
    }

    #[test]
    fn slow_start_output_is_short_and_names_the_phase() {
        let mut output = Vec::new();
        write_slow_start(&mut output, StartupPhase::ReadingRepositoryState).unwrap();
        write_phase(&mut output, StartupPhase::PreparingInterface).unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "Diffo is still starting after 3 seconds.\n\
             [startup] reading Git status and diffs\n\
             [startup] preparing the interface\n"
        );
    }
}
