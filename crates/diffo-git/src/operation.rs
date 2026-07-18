use std::{
    env, io,
    os::unix::process::CommandExt as _,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use diffo_core::{
    CancellationHandle, FailureKind, OperationFailure, OperationOutcome, OperationResult,
    RepositoryAction, RepositoryOperationContext, RepositorySource, UpstreamState,
};
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};

use super::{
    GitRepositorySource,
    askpass::{ASKPASS_MARKER, ASKPASS_SOCKET, AskpassBridge},
};

impl GitRepositorySource {
    pub(super) fn apply_operation(
        &self,
        action: &RepositoryAction,
        context: Option<&RepositoryOperationContext>,
    ) -> std::result::Result<OperationOutcome, OperationFailure> {
        let default_cancellation = CancellationHandle::default();
        let cancellation = context.map_or(&default_cancellation, |context| &context.cancellation);
        if matches!(
            action,
            RepositoryAction::Fetch | RepositoryAction::Pull | RepositoryAction::Push
        ) && let Some(delay) = e2e_network_delay()
            && !cancellable_delay(delay, cancellation)
        {
            return Ok(OperationOutcome::Cancelled);
        }

        if cancellation.is_cancelled() {
            return Ok(OperationOutcome::Cancelled);
        }

        let before_head = matches!(action, RepositoryAction::Pull)
            .then(|| self.git(&["rev-parse", "HEAD"]))
            .transpose()
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?
            .map(|head| String::from_utf8_lossy(&head).trim().to_owned());
        let before_fetch = matches!(action, RepositoryAction::Fetch)
            .then(|| self.snapshot().ok().and_then(|snapshot| snapshot.upstream))
            .flatten();
        if matches!(action, RepositoryAction::Push)
            && self
                .snapshot()
                .ok()
                .and_then(|snapshot| snapshot.upstream)
                .is_some_and(|upstream| upstream.behind > 0)
        {
            return Err(operation_failure(
                action,
                FailureKind::PullRequired,
                "pull required before push",
            ));
        }

        let mut command = Command::new("git");
        command
            .current_dir(&self.root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .stdin(Stdio::null());
        match action {
            RepositoryAction::Stage(path) => {
                command.args(["add", "--"]).arg(path);
            }
            RepositoryAction::Unstage(path) => {
                command.args(["reset", "--"]).arg(path);
            }
            RepositoryAction::StageAll => {
                command.args(["add", "--all"]);
            }
            RepositoryAction::UnstageAll => {
                command.arg("reset");
            }
            RepositoryAction::Fetch => {
                command.arg("fetch");
            }
            RepositoryAction::Pull => {
                command.args(["pull", "--ff-only"]);
            }
            RepositoryAction::Push => {
                command.args(["push", "--porcelain"]);
            }
            RepositoryAction::Commit(message) => {
                command.args(["commit", "-m", message]);
            }
        }

        let _bridge = configure_askpass(&mut command, action, context, self.askpass_executable())?;
        let outcome = run_cancellable(&mut command, cancellation)
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        let output = match outcome {
            CommandOutcome::Output(output) => output,
            CommandOutcome::Cancelled => return Ok(OperationOutcome::Cancelled),
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(classify_failure(action, &format!("{stdout}\n{stderr}")));
        }
        collect_operation_result(self, action, before_head, before_fetch.as_ref())
            .map(OperationOutcome::Completed)
    }
}

fn cancellable_delay(duration: Duration, cancellation: &CancellationHandle) -> bool {
    let deadline = std::time::Instant::now() + duration;
    while std::time::Instant::now() < deadline {
        if cancellation.is_cancelled() {
            return false;
        }
        thread::sleep(
            deadline
                .saturating_duration_since(std::time::Instant::now())
                .min(Duration::from_millis(10)),
        );
    }
    !cancellation.is_cancelled()
}

fn configure_askpass(
    command: &mut Command,
    action: &RepositoryAction,
    context: Option<&RepositoryOperationContext>,
    askpass_executable: Option<&std::path::Path>,
) -> std::result::Result<Option<AskpassBridge>, OperationFailure> {
    if !matches!(
        action,
        RepositoryAction::Fetch | RepositoryAction::Pull | RepositoryAction::Push
    ) {
        return Ok(None);
    }
    let bridge = context
        .map(AskpassBridge::start)
        .transpose()
        .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
    if let Some(bridge) = bridge.as_ref() {
        let executable = askpass_executable.ok_or_else(|| {
            operation_failure(
                action,
                FailureKind::Unknown,
                "prepared askpass executable is unavailable",
            )
        })?;
        command
            .env("GIT_ASKPASS", executable)
            .env("SSH_ASKPASS", executable)
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env(ASKPASS_MARKER, "1")
            .env(ASKPASS_SOCKET, bridge.socket());
    }
    Ok(bridge)
}

enum CommandOutcome {
    Output(Output),
    Cancelled,
}

fn run_cancellable(
    command: &mut Command,
    cancellation: &CancellationHandle,
) -> io::Result<CommandOutcome> {
    command
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().map(read_output);
    let stderr = child.stderr.take().map(read_output);
    let mut was_cancelled = false;
    let status = loop {
        if cancellation.is_cancelled() && !was_cancelled {
            was_cancelled = true;
            signal_process_group(child.id(), Signal::SIGTERM);
        }
        if let Some(status) = child.try_wait()? {
            was_cancelled |= cancellation.is_cancelled();
            break status;
        }
        if was_cancelled {
            let deadline = Instant::now() + Duration::from_millis(200);
            while Instant::now() < deadline {
                if let Some(status) = child.try_wait()? {
                    let _ = join_output(stdout);
                    let _ = join_output(stderr);
                    let _ = status;
                    return Ok(CommandOutcome::Cancelled);
                }
                thread::sleep(Duration::from_millis(10));
            }
            signal_process_group(child.id(), Signal::SIGKILL);
            let _ = child.wait();
            let _ = join_output(stdout);
            let _ = join_output(stderr);
            return Ok(CommandOutcome::Cancelled);
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = join_output(stdout)?;
    let stderr = join_output(stderr)?;
    if was_cancelled {
        return Ok(CommandOutcome::Cancelled);
    }
    Ok(CommandOutcome::Output(Output {
        status,
        stdout,
        stderr,
    }))
}

fn read_output(
    mut pipe: impl io::Read + Send + 'static,
) -> thread::JoinHandle<io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut output = Vec::new();
        pipe.read_to_end(&mut output)?;
        Ok(output)
    })
}

fn join_output(reader: Option<thread::JoinHandle<io::Result<Vec<u8>>>>) -> io::Result<Vec<u8>> {
    reader.map_or_else(
        || Ok(Vec::new()),
        |reader| {
            reader
                .join()
                .map_err(|_| io::Error::other("Git output reader stopped"))?
        },
    )
}

fn signal_process_group(id: u32, signal: Signal) {
    if let Ok(id) = i32::try_from(id) {
        let _ = killpg(Pid::from_raw(id), signal);
    }
}

fn collect_operation_result(
    source: &GitRepositorySource,
    action: &RepositoryAction,
    before_head: Option<String>,
    before_fetch: Option<&UpstreamState>,
) -> std::result::Result<OperationResult, OperationFailure> {
    match action {
        RepositoryAction::Stage(_) | RepositoryAction::StageAll => Ok(OperationResult::Stage),
        RepositoryAction::Unstage(_) | RepositoryAction::UnstageAll => Ok(OperationResult::Unstage),
        RepositoryAction::Fetch => {
            let after = source
                .snapshot()
                .ok()
                .and_then(|snapshot| snapshot.upstream);
            let updated_refs = usize::from(before_fetch != after.as_ref());
            Ok(OperationResult::Fetch { updated_refs })
        }
        RepositoryAction::Pull => {
            let old = before_head.unwrap_or_default();
            let new = source
                .git(&["rev-parse", "HEAD"])
                .map_err(|error| {
                    operation_failure(action, FailureKind::Unknown, &error.to_string())
                })
                .map(|head| String::from_utf8_lossy(&head).trim().to_owned())?;
            let range = format!("{old}..{new}");
            let commits = source
                .git(&["rev-list", "--count", &range])
                .ok()
                .and_then(|count| String::from_utf8(count).ok())
                .and_then(|count| count.trim().parse().ok())
                .unwrap_or(0);
            Ok(OperationResult::Pull { commits })
        }
        RepositoryAction::Push => {
            let snapshot = source.snapshot().map_err(|error| {
                operation_failure(action, FailureKind::Unknown, &error.to_string())
            })?;
            let hash = snapshot
                .recent_commits
                .first()
                .map_or_else(|| "unknown".to_owned(), |commit| commit.id.clone());
            let upstream = snapshot
                .upstream
                .map_or_else(|| "upstream".to_owned(), |upstream| upstream.name);
            Ok(OperationResult::Push { hash, upstream })
        }
        RepositoryAction::Commit(_) => {
            let hash = source
                .git(&["rev-parse", "HEAD"])
                .map_err(|error| {
                    operation_failure(action, FailureKind::Unknown, &error.to_string())
                })
                .map(|head| String::from_utf8_lossy(&head).trim().to_owned())?;
            Ok(OperationResult::Commit { hash })
        }
    }
}

fn operation_failure(
    action: &RepositoryAction,
    kind: FailureKind,
    detail: &str,
) -> OperationFailure {
    OperationFailure {
        action: action.clone(),
        kind,
        detail: detail.to_owned(),
    }
}

pub(super) fn classify_failure(action: &RepositoryAction, output: &str) -> OperationFailure {
    let text = output.to_ascii_lowercase();
    let (kind, detail) = if text.contains("non-fast-forward")
        || text.contains("fetch first")
        || text.contains("remote contains work")
    {
        (FailureKind::PushRejected, "remote changed; pull required")
    } else if text.contains("not possible to fast-forward") || text.contains("divergent branches") {
        (
            FailureKind::Diverged,
            "branches diverged; merge or rebase required",
        )
    } else if text.contains("hook declined") || text.contains("pre-receive hook") {
        (FailureKind::HookRejected, "rejected by remote hook")
    } else if text.contains("authentication")
        || text.contains("permission denied")
        || text.contains("could not read username")
    {
        (FailureKind::Authentication, "authentication required")
    } else if text.contains("conflict") {
        (FailureKind::MergeConflict, "resolve repository conflicts")
    } else if text.contains("no configured push destination")
        || text.contains("does not appear to be a git repository")
        || text.contains("no such remote")
    {
        (FailureKind::NoRemote, "no remote configured")
    } else if text.contains("could not resolve host")
        || text.contains("connection refused")
        || text.contains("unable to access")
    {
        (FailureKind::Network, "network unavailable")
    } else if text.contains("local changes") || text.contains("would be overwritten") {
        (
            FailureKind::DirtyWorktree,
            "local changes block the operation",
        )
    } else {
        (FailureKind::Unknown, "Git operation failed")
    };
    operation_failure(action, kind, detail)
}

fn e2e_network_delay() -> Option<Duration> {
    let milliseconds = env::var("DIFFO_E2E_NETWORK_DELAY_MS")
        .ok()?
        .parse::<u64>()
        .ok()?
        .min(2_000);
    Some(Duration::from_millis(milliseconds))
}

#[cfg(test)]
mod cancellation_tests {
    use nix::{errno::Errno, sys::signal::kill};

    use super::*;

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
}
