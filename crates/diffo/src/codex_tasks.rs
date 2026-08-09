use std::{
    env,
    ffi::{OsStr, OsString},
    io::{self, Read, Write as _},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread,
    time::{Duration, Instant},
};

use diffo_ai_config::{
    AI_COMMIT_MODEL, AI_COMMIT_PROMPT, AI_COMMIT_SCHEMA, AI_REVIEW_BATCH_CHANGES, AI_REVIEW_MODEL,
    AI_REVIEW_PROMPT, AI_REVIEW_SCHEMA, CODEX_EXECUTABLE, CODEX_SANDBOX, MAX_CODEX_OUTPUT_BYTES,
    MAX_CODEX_RUNTIME_SECONDS,
};
use diffo_app::{
    review::{
        AttentionCategory, CodexAvailability, ReviewCodexOutcome, ReviewCodexTaskResult,
        ReviewRequest, ReviewStop,
    },
    workbench::{AiCommitOutcome, AiCommitRequest, Workbench},
};
use diffo_core::{ApplicationCommandId, CancellationHandle, FailureKind, OperationFailure};
use diffo_repository_service::RepositoryService;
use serde::Deserialize;
use tempfile::NamedTempFile;

use crate::codex_failure;

static RESOLVED_CODEX_EXECUTABLE: OnceLock<OsString> = OnceLock::new();

enum CodexExecutable {
    Found(OsString),
    Missing(String),
}

enum CodexTaskResult {
    AiCommit(ApplicationCommandId, AiCommitOutcome),
    Review(ReviewCodexTaskResult),
}

struct BusyGuard(Arc<AtomicBool>);

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(crate) struct CodexTasks {
    repository_root: PathBuf,
    executable: Result<OsString, String>,
    busy: Arc<AtomicBool>,
    sender: Sender<CodexTaskResult>,
    receiver: Receiver<CodexTaskResult>,
}

impl CodexTasks {
    pub(crate) fn new(repository_root: PathBuf) -> Self {
        let (sender, receiver) = channel();
        let executable = match selected_codex_executable() {
            CodexExecutable::Found(executable) => Ok(executable),
            CodexExecutable::Missing(error) => Err(error),
        };
        Self {
            repository_root,
            executable,
            busy: Arc::new(AtomicBool::new(false)),
            sender,
            receiver,
        }
    }

    pub(crate) fn availability(&self) -> CodexAvailability {
        match &self.executable {
            Ok(_) => CodexAvailability::Available,
            Err(error) => CodexAvailability::Unavailable(error.clone()),
        }
    }

    pub(crate) fn start(
        &self,
        id: ApplicationCommandId,
        request: AiCommitRequest,
        cancellation: CancellationHandle,
    ) -> Result<(), String> {
        let executable = self.executable.as_ref().map_err(Clone::clone)?;
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err("Codex is already handling another AI request".to_owned());
        }
        let sender = self.sender.clone();
        let repository_root = self.repository_root.clone();
        let executable = executable.clone();
        let busy = Arc::clone(&self.busy);
        let spawn = thread::Builder::new()
            .name("codex-ai".to_owned())
            .spawn(move || {
                let _busy = BusyGuard(busy);
                let outcome = run_codex(&executable, &repository_root, &request, &cancellation);
                let _ = sender.send(CodexTaskResult::AiCommit(id, outcome));
            });
        if let Err(error) = spawn {
            self.busy.store(false, Ordering::Release);
            return Err(format!("Could not start the Codex worker: {error}"));
        }
        Ok(())
    }

    pub(crate) fn start_review(
        &self,
        id: ApplicationCommandId,
        request: ReviewRequest,
        cancellation: CancellationHandle,
    ) -> Result<(), String> {
        let executable = self.executable.as_ref().map_err(Clone::clone)?;
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err("Codex is already handling another AI request".to_owned());
        }
        let sender = self.sender.clone();
        let repository_root = self.repository_root.clone();
        let executable = executable.clone();
        let busy = Arc::clone(&self.busy);
        let spawn = thread::Builder::new()
            .name("codex-ai".to_owned())
            .spawn(move || {
                let _busy = BusyGuard(busy);
                let batches = request.batches(AI_REVIEW_BATCH_CHANGES);
                let started = Instant::now();
                let mut stops = 0_usize;
                for (index, batch) in batches.iter().enumerate() {
                    let timeout = Duration::from_secs(MAX_CODEX_RUNTIME_SECONDS)
                        .saturating_sub(started.elapsed());
                    let outcome = if timeout.is_zero() {
                        ReviewCodexOutcome::Failed(timeout_message())
                    } else {
                        run_review_codex(
                            &executable,
                            &repository_root,
                            batch,
                            &cancellation,
                            timeout,
                        )
                    };
                    if let ReviewCodexOutcome::Generated(review) = &outcome {
                        stops = stops.saturating_add(review.stops.len());
                    }
                    let complete = index + 1 == batches.len()
                        || stops >= 8
                        || !matches!(outcome, ReviewCodexOutcome::Generated(_));
                    if sender
                        .send(CodexTaskResult::Review(ReviewCodexTaskResult {
                            id,
                            outcome,
                            complete,
                        }))
                        .is_err()
                        || complete
                    {
                        break;
                    }
                }
            });
        if let Err(error) = spawn {
            self.busy.store(false, Ordering::Release);
            return Err(format!("Could not start the Codex worker: {error}"));
        }
        Ok(())
    }

    pub(crate) fn drain(&self, workbench: &mut Workbench, repository_service: &RepositoryService) {
        while let Ok(result) = self.receiver.try_recv() {
            match result {
                CodexTaskResult::AiCommit(id, outcome) => {
                    let Some(handoff) = workbench.ai_commit_finished(id, outcome) else {
                        continue;
                    };
                    if !repository_service.execute(id, handoff.action.clone(), handoff.cancellation)
                    {
                        workbench.action_failed(
                            id,
                            OperationFailure {
                                action: handoff.action,
                                kind: FailureKind::Unknown,
                                detail: "repository service is unavailable".to_owned(),
                            },
                        );
                    }
                }
                CodexTaskResult::Review(result) => workbench.accept_review_codex_result(result),
            }
        }
    }
}

fn selected_codex_executable() -> CodexExecutable {
    if let Some(executable) = RESOLVED_CODEX_EXECUTABLE.get() {
        return CodexExecutable::Found(executable.to_owned());
    }
    let resolved = resolve_configured_codex();
    match resolved {
        Some(executable) => {
            let executable = RESOLVED_CODEX_EXECUTABLE.get_or_init(|| executable);
            CodexExecutable::Found(executable.to_owned())
        }
        None => CodexExecutable::Missing(
            "Codex CLI was not found in this environment. Install Codex, sign in, and restart Diffo."
                .to_owned(),
        ),
    }
}

fn resolve_configured_codex() -> Option<OsString> {
    let inherited_path = env::var_os("PATH");
    let shell = env::var_os("SHELL").unwrap_or_else(|| OsString::from("/bin/sh"));
    resolve_executable(CODEX_EXECUTABLE, inherited_path.as_deref(), &shell)
}

fn resolve_executable(
    executable: &str,
    inherited_path: Option<&OsStr>,
    shell: &OsStr,
) -> Option<OsString> {
    inherited_path
        .and_then(|path| {
            env::split_paths(path)
                .map(|directory| directory.join(executable))
                .find(|candidate| candidate.is_file())
        })
        .map(OsString::from)
        .or_else(|| resolve_from_login_shell(executable, shell))
}

fn resolve_from_login_shell(executable: &str, shell: &OsStr) -> Option<OsString> {
    let output = Command::new(shell)
        .args(["-lc", &format!("command -v -- {executable}")])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let path = stdout.lines().rev().find(|line| !line.trim().is_empty())?;
    let path = Path::new(path.trim());
    (path.is_absolute() && path.is_file()).then(|| path.as_os_str().to_owned())
}

fn run_codex(
    executable: &OsStr,
    repository_root: &Path,
    request: &AiCommitRequest,
    cancellation: &CancellationHandle,
) -> AiCommitOutcome {
    let repository_name = repository_root
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("repository");
    let context = request.prompt_context(repository_name);
    match run_codex_raw(
        executable,
        repository_root,
        CodexRequest {
            model: AI_COMMIT_MODEL,
            schema: AI_COMMIT_SCHEMA,
            prompt: AI_COMMIT_PROMPT,
            context: &context,
        },
        cancellation,
        Duration::from_secs(MAX_CODEX_RUNTIME_SECONDS),
    ) {
        RawCodexOutcome::Completed(bytes) => parse_response(&bytes),
        RawCodexOutcome::Failed(error) => AiCommitOutcome::Failed(error),
        RawCodexOutcome::Cancelled => AiCommitOutcome::Cancelled,
    }
}

fn run_review_codex(
    executable: &OsStr,
    repository_root: &Path,
    request: &ReviewRequest,
    cancellation: &CancellationHandle,
    timeout: Duration,
) -> ReviewCodexOutcome {
    let repository_name = repository_root
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("repository");
    let context = request.prompt_context(repository_name);
    match run_codex_raw(
        executable,
        repository_root,
        CodexRequest {
            model: AI_REVIEW_MODEL,
            schema: AI_REVIEW_SCHEMA,
            prompt: AI_REVIEW_PROMPT,
            context: &context,
        },
        cancellation,
        timeout,
    ) {
        RawCodexOutcome::Completed(bytes) => parse_review_response(request, &bytes),
        RawCodexOutcome::Failed(error) => ReviewCodexOutcome::Failed(error),
        RawCodexOutcome::Cancelled => ReviewCodexOutcome::Cancelled,
    }
}

enum RawCodexOutcome {
    Completed(Vec<u8>),
    Failed(String),
    Cancelled,
}

enum WaitOutcome {
    Completed(std::process::ExitStatus),
    Cancelled,
    TimedOut,
    Failed(io::Error),
}

#[derive(Clone, Copy)]
struct CodexRequest<'a> {
    model: &'a str,
    schema: &'a str,
    prompt: &'a str,
    context: &'a str,
}

fn run_codex_raw(
    executable: &OsStr,
    repository_root: &Path,
    request: CodexRequest<'_>,
    cancellation: &CancellationHandle,
    timeout: Duration,
) -> RawCodexOutcome {
    if cancellation.is_cancelled() {
        return RawCodexOutcome::Cancelled;
    }
    let (_schema, mut child) = match spawn_codex(
        executable,
        repository_root,
        request.model,
        request.schema,
        request.prompt,
    ) {
        Ok(process) => process,
        Err(error) => return RawCodexOutcome::Failed(error),
    };
    let stdout = child
        .stdout
        .take()
        .map(|pipe| read_in_background(pipe, false));
    let stderr = child
        .stderr
        .take()
        .map(|pipe| read_in_background(pipe, true));
    let input = child
        .stdin
        .take()
        .map(|input| write_in_background(input, request.context.as_bytes().to_vec()));
    let status = match wait_for_child(&mut child, cancellation, timeout) {
        WaitOutcome::Completed(status) => status,
        WaitOutcome::Cancelled => {
            terminate_child(&mut child);
            let _ = join_input(input);
            let _ = join_output(stdout);
            let _ = join_output(stderr);
            return RawCodexOutcome::Cancelled;
        }
        WaitOutcome::TimedOut => {
            terminate_child(&mut child);
            let _ = join_input(input);
            let _ = join_output(stdout);
            let _ = join_output(stderr);
            return timeout_failure();
        }
        WaitOutcome::Failed(error) => {
            terminate_child(&mut child);
            let _ = join_input(input);
            let _ = join_output(stdout);
            let _ = join_output(stderr);
            return RawCodexOutcome::Failed(format!("Could not wait for Codex CLI: {error}"));
        }
    };
    terminate_descendants(child.id());
    let input = join_input(input);
    let stdout = match join_output(stdout) {
        Ok(output) => output,
        Err(error) => {
            return RawCodexOutcome::Failed(format!("Could not read the Codex response: {error}"));
        }
    };
    let stderr = join_output(stderr);
    if !status.success() {
        return RawCodexOutcome::Failed(match stderr {
            Ok(stderr) => codex_failure::process_failure(status, &stderr.bytes, stderr.truncated),
            Err(_) => codex_failure::process_failure(status, &[], false),
        });
    }
    if let Err(error) = input {
        return RawCodexOutcome::Failed(format!("Could not send changes to Codex: {error}"));
    }
    if let Err(error) = stderr {
        return RawCodexOutcome::Failed(format!("Could not read Codex progress: {error}"));
    }
    finish_codex(&stdout)
}

fn spawn_codex(
    executable: &OsStr,
    repository_root: &Path,
    model: &str,
    schema_text: &str,
    prompt: &str,
) -> Result<(NamedTempFile, Child), String> {
    let mut schema = match NamedTempFile::new() {
        Ok(schema) => schema,
        Err(error) => {
            return Err(format!(
                "Could not create the Codex response schema: {error}"
            ));
        }
    };
    if let Err(error) = schema.write_all(schema_text.as_bytes()) {
        return Err(format!(
            "Could not write the Codex response schema: {error}"
        ));
    }
    let mut command = Command::new(executable);
    command
        .current_dir(repository_root)
        .args(["exec", "--ephemeral", "--model", model])
        .args(["--sandbox", CODEX_SANDBOX, "--output-schema"])
        .arg(schema.path())
        .arg(prompt)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;

        command.process_group(0);
    }
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(
                "The Codex CLI disappeared after Diffo found it. Restart Diffo to check the installation again."
                    .to_owned(),
            );
        }
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            return Err(
                "Codex CLI is not executable. Fix its file permissions, then restart Diffo."
                    .to_owned(),
            );
        }
        Err(error) => {
            return Err(format!("Could not start Codex CLI: {error}"));
        }
    };
    Ok((schema, child))
}

fn timeout_failure() -> RawCodexOutcome {
    RawCodexOutcome::Failed(timeout_message())
}

fn timeout_message() -> String {
    format!("Codex did not finish within {MAX_CODEX_RUNTIME_SECONDS} seconds. Try again.")
}

fn wait_for_child(
    child: &mut Child,
    cancellation: &CancellationHandle,
    timeout: Duration,
) -> WaitOutcome {
    let started = Instant::now();
    loop {
        if cancellation.is_cancelled() {
            return WaitOutcome::Cancelled;
        }
        if started.elapsed() >= timeout {
            return WaitOutcome::TimedOut;
        }
        match child.try_wait() {
            Ok(Some(status)) => return WaitOutcome::Completed(status),
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(error) => return WaitOutcome::Failed(error),
        }
    }
}

struct BoundedOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

fn read_in_background(
    mut pipe: impl Read + Send + 'static,
    keep_tail: bool,
) -> thread::JoinHandle<io::Result<BoundedOutput>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4096];
        let mut truncated = false;
        loop {
            let count = pipe.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            if keep_tail {
                bytes.extend_from_slice(&buffer[..count]);
                let excess = bytes.len().saturating_sub(MAX_CODEX_OUTPUT_BYTES);
                bytes.drain(..excess);
                truncated |= excess > 0;
            } else {
                let remaining = MAX_CODEX_OUTPUT_BYTES.saturating_sub(bytes.len());
                bytes.extend_from_slice(&buffer[..count.min(remaining)]);
                truncated |= count > remaining;
            }
        }
        Ok(BoundedOutput { bytes, truncated })
    })
}

fn write_in_background(
    mut input: ChildStdin,
    context: Vec<u8>,
) -> thread::JoinHandle<io::Result<()>> {
    thread::spawn(move || input.write_all(&context))
}

fn join_input(writer: Option<thread::JoinHandle<io::Result<()>>>) -> io::Result<()> {
    writer.map_or(Ok(()), |writer| {
        writer
            .join()
            .map_err(|_| io::Error::other("Codex input writer stopped"))?
    })
}

fn join_output(
    reader: Option<thread::JoinHandle<io::Result<BoundedOutput>>>,
) -> io::Result<BoundedOutput> {
    reader.map_or_else(
        || {
            Ok(BoundedOutput {
                bytes: Vec::new(),
                truncated: false,
            })
        },
        |reader| {
            reader
                .join()
                .map_err(|_| io::Error::other("Codex output reader stopped"))?
        },
    )
}

fn finish_codex(stdout: &BoundedOutput) -> RawCodexOutcome {
    if stdout.truncated {
        return RawCodexOutcome::Failed("Codex returned an oversized response".to_owned());
    }
    RawCodexOutcome::Completed(stdout.bytes.clone())
}

fn terminate_child(child: &mut Child) {
    terminate_process_group(child.id());
    let _ = child.kill();
    let _ = child.wait();
}

fn terminate_descendants(id: u32) {
    terminate_process_group(id);
}

#[cfg(unix)]
fn terminate_process_group(id: u32) {
    use nix::{
        sys::signal::{Signal, killpg},
        unistd::Pid,
    };

    if let Ok(id) = i32::try_from(id) {
        let _ = killpg(Pid::from_raw(id), Signal::SIGKILL);
    }
}

#[cfg(not(unix))]
fn terminate_process_group(_id: u32) {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommitResponse {
    subject: String,
}

fn parse_response(bytes: &[u8]) -> AiCommitOutcome {
    if response_is_empty(bytes) {
        return AiCommitOutcome::Failed("Codex returned no commit message".to_owned());
    }
    let response = match serde_json::from_slice::<CommitResponse>(bytes) {
        Ok(response) => response,
        Err(error) => {
            return AiCommitOutcome::Failed(format!(
                "Codex returned an invalid commit message: {error}"
            ));
        }
    };
    let subject = response.subject.trim();
    let valid = !subject.is_empty()
        && subject.chars().count() <= 72
        && !subject.chars().any(char::is_control)
        && subject == response.subject;
    if !valid {
        return AiCommitOutcome::Failed(
            "Codex returned a commit subject that was empty, multiline, padded, or over 72 characters"
                .to_owned(),
        );
    }
    AiCommitOutcome::Generated(response.subject)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewResponse {
    overview: Vec<String>,
    stops: Vec<ReviewStopResponse>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewStopResponse {
    title: String,
    category: String,
    reason: String,
    primary_hunk_id: String,
}

fn parse_review_response(request: &ReviewRequest, bytes: &[u8]) -> ReviewCodexOutcome {
    if response_is_empty(bytes) {
        return ReviewCodexOutcome::Failed("Codex returned no review".to_owned());
    }
    let response = match serde_json::from_slice::<ReviewResponse>(bytes) {
        Ok(response) => response,
        Err(error) => {
            return ReviewCodexOutcome::Failed(format!(
                "Codex returned an invalid review: {error}"
            ));
        }
    };
    let mut stops = Vec::with_capacity(response.stops.len());
    for stop in response.stops {
        let Some(category) = AttentionCategory::parse(&stop.category) else {
            return ReviewCodexOutcome::Failed(
                "Codex returned an unknown review category".to_owned(),
            );
        };
        stops.push(ReviewStop {
            title: stop.title,
            category,
            reason: stop.reason,
            primary_hunk_id: stop.primary_hunk_id,
        });
    }
    request
        .validate_review(response.overview, stops)
        .map_or_else(
            || {
                ReviewCodexOutcome::Failed(
                    "Codex returned a review that could not be matched to the current changes"
                        .to_owned(),
                )
            },
            ReviewCodexOutcome::Generated,
        )
}

fn response_is_empty(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii_whitespace)
}
#[cfg(test)]
mod tests;
