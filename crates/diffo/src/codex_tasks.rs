use std::{
    env,
    ffi::{OsStr, OsString},
    io::{self, Read, Write as _},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread,
    time::Duration,
};

use diffo_ai_config::{
    AI_COMMIT_MODEL, AI_COMMIT_PROMPT, AI_COMMIT_SCHEMA, AI_REVIEW_ASK_PROMPT,
    AI_REVIEW_ASK_SCHEMA, AI_REVIEW_MODEL, AI_REVIEW_PROMPT, AI_REVIEW_SCHEMA, CODEX_EXECUTABLE,
    CODEX_SANDBOX, MAX_CODEX_OUTPUT_BYTES,
};
use diffo_app::{
    review::{
        AskRequest, AttentionCategory, CodexAvailability, ReviewCodexOutcome, ReviewCodexRequest,
        ReviewCodexTask, ReviewCodexTaskResult, ReviewRequest, ReviewStop,
    },
    workbench::{AiCommitOutcome, AiCommitRequest, Workbench},
};
use diffo_core::{ApplicationCommandId, CancellationHandle, FailureKind, OperationFailure};
use diffo_repository_service::RepositoryService;
use serde::Deserialize;
use tempfile::NamedTempFile;

static RESOLVED_CODEX_EXECUTABLE: OnceLock<OsString> = OnceLock::new();

enum CodexExecutable {
    Found(OsString),
    Missing(String),
}

enum CodexTaskResult {
    AiCommit(ApplicationCommandId, AiCommitOutcome),
    Review(ReviewCodexTaskResult),
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
    ) -> bool {
        let Ok(executable) = &self.executable else {
            return false;
        };
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        let sender = self.sender.clone();
        let repository_root = self.repository_root.clone();
        let executable = executable.clone();
        let busy = Arc::clone(&self.busy);
        thread::spawn(move || {
            let outcome = run_codex(&executable, &repository_root, &request, &cancellation);
            busy.store(false, Ordering::Release);
            let _ = sender.send(CodexTaskResult::AiCommit(id, outcome));
        });
        true
    }

    pub(crate) fn start_review(&self, task: ReviewCodexTask) -> bool {
        let Ok(executable) = &self.executable else {
            return false;
        };
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        let sender = self.sender.clone();
        let repository_root = self.repository_root.clone();
        let executable = executable.clone();
        let busy = Arc::clone(&self.busy);
        thread::spawn(move || {
            let outcome = run_review_codex(
                &executable,
                &repository_root,
                &task.request,
                &task.cancellation,
            );
            busy.store(false, Ordering::Release);
            let _ = sender.send(CodexTaskResult::Review(ReviewCodexTaskResult {
                id: task.id,
                outcome,
            }));
        });
        true
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
        None => CodexExecutable::Missing(format!(
            "Codex CLI was not found in this environment. Install Codex, sign in, and restart Diffo."
        )),
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
        AI_COMMIT_MODEL,
        AI_COMMIT_SCHEMA,
        AI_COMMIT_PROMPT,
        &context,
        cancellation,
    ) {
        RawCodexOutcome::Completed(bytes) => parse_response(&bytes),
        RawCodexOutcome::Failed(error) => AiCommitOutcome::Failed(error),
        RawCodexOutcome::Cancelled => AiCommitOutcome::Cancelled,
    }
}

fn run_review_codex(
    executable: &OsStr,
    repository_root: &Path,
    request: &ReviewCodexRequest,
    cancellation: &CancellationHandle,
) -> ReviewCodexOutcome {
    let repository_name = repository_root
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("repository");
    let (schema, prompt, context) = match request {
        ReviewCodexRequest::Generate(request) => (
            AI_REVIEW_SCHEMA,
            AI_REVIEW_PROMPT,
            request.prompt_context(repository_name),
        ),
        ReviewCodexRequest::Ask(request) => (
            AI_REVIEW_ASK_SCHEMA,
            AI_REVIEW_ASK_PROMPT,
            request.prompt_context(repository_name),
        ),
    };
    match run_codex_raw(
        executable,
        repository_root,
        AI_REVIEW_MODEL,
        schema,
        prompt,
        &context,
        cancellation,
    ) {
        RawCodexOutcome::Completed(bytes) => match request {
            ReviewCodexRequest::Generate(request) => parse_review_response(request, &bytes),
            ReviewCodexRequest::Ask(request) => parse_ask_response(request, &bytes),
        },
        RawCodexOutcome::Failed(error) => ReviewCodexOutcome::Failed(error),
        RawCodexOutcome::Cancelled => ReviewCodexOutcome::Cancelled,
    }
}

enum RawCodexOutcome {
    Completed(Vec<u8>),
    Failed(String),
    Cancelled,
}

fn run_codex_raw(
    executable: &OsStr,
    repository_root: &Path,
    model: &str,
    schema_text: &str,
    prompt: &str,
    context: &str,
    cancellation: &CancellationHandle,
) -> RawCodexOutcome {
    if cancellation.is_cancelled() {
        return RawCodexOutcome::Cancelled;
    }
    let mut schema = match NamedTempFile::new() {
        Ok(schema) => schema,
        Err(error) => {
            return RawCodexOutcome::Failed(format!(
                "Could not create the Codex response schema: {error}"
            ));
        }
    };
    if let Err(error) = schema.write_all(schema_text.as_bytes()) {
        return RawCodexOutcome::Failed(format!(
            "Could not write the Codex response schema: {error}"
        ));
    }

    let mut child = match Command::new(executable)
        .current_dir(repository_root)
        .args(["exec", "--ephemeral", "--model", model])
        .args(["--sandbox", CODEX_SANDBOX, "--output-schema"])
        .arg(schema.path())
        .arg(prompt)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return RawCodexOutcome::Failed(
                "The Codex CLI disappeared after Diffo found it. Restart Diffo to check the installation again."
                    .to_owned(),
            );
        }
        Err(error) => {
            return RawCodexOutcome::Failed(format!("Could not start Codex CLI: {error}"));
        }
    };

    let stdout = child.stdout.take().map(read_in_background);
    let stderr = child.stderr.take().map(read_in_background);
    let write_result = child
        .stdin
        .take()
        .map_or(Ok(()), |mut input| input.write_all(context.as_bytes()));
    if let Err(error) = write_result {
        let _ = child.kill();
        let _ = child.wait();
        let _ = join_output(stdout);
        let _ = join_output(stderr);
        return RawCodexOutcome::Failed(format!("Could not send changes to Codex: {error}"));
    }

    let status = loop {
        if cancellation.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = join_output(stdout);
            let _ = join_output(stderr);
            return RawCodexOutcome::Cancelled;
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = join_output(stdout);
                let _ = join_output(stderr);
                return RawCodexOutcome::Failed(format!("Could not wait for Codex CLI: {error}"));
            }
        }
    };

    let stdout = match join_output(stdout) {
        Ok(output) => output,
        Err(error) => {
            return RawCodexOutcome::Failed(format!("Could not read the Codex response: {error}"));
        }
    };
    let stderr = match join_output(stderr) {
        Ok(output) => output,
        Err(error) => {
            return RawCodexOutcome::Failed(format!("Could not read Codex progress: {error}"));
        }
    };
    finish_codex(status, &stdout, &stderr)
}

struct BoundedOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

fn read_in_background(
    mut pipe: impl Read + Send + 'static,
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
            let remaining = MAX_CODEX_OUTPUT_BYTES.saturating_sub(bytes.len());
            bytes.extend_from_slice(&buffer[..count.min(remaining)]);
            truncated |= count > remaining;
        }
        Ok(BoundedOutput { bytes, truncated })
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

fn finish_codex(
    status: ExitStatus,
    stdout: &BoundedOutput,
    stderr: &BoundedOutput,
) -> RawCodexOutcome {
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr.bytes).trim().to_owned();
        return RawCodexOutcome::Failed(if detail.is_empty() {
            format!("Codex CLI exited with {status}")
        } else if stderr.truncated {
            format!("{detail}\n…")
        } else {
            detail
        });
    }
    if stdout.truncated {
        return RawCodexOutcome::Failed("Codex returned an oversized response".to_owned());
    }
    RawCodexOutcome::Completed(stdout.bytes.clone())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommitResponse {
    subject: String,
}

fn parse_response(bytes: &[u8]) -> AiCommitOutcome {
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
    related_hunk_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AskResponse {
    text: Vec<String>,
    hunk_ids: Vec<String>,
}

fn parse_review_response(request: &ReviewRequest, bytes: &[u8]) -> ReviewCodexOutcome {
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
            related_hunk_ids: stop.related_hunk_ids,
        });
    }
    request
        .validate_review(response.overview, stops)
        .map_or_else(
            || {
                ReviewCodexOutcome::Failed(
                    "Codex returned invalid or unknown hunk references".to_owned(),
                )
            },
            ReviewCodexOutcome::Generated,
        )
}

fn parse_ask_response(request: &AskRequest, bytes: &[u8]) -> ReviewCodexOutcome {
    let response = match serde_json::from_slice::<AskResponse>(bytes) {
        Ok(response) => response,
        Err(error) => {
            return ReviewCodexOutcome::Failed(format!(
                "Codex returned an invalid answer: {error}"
            ));
        }
    };
    request
        .validate_answer(response.text, response.hunk_ids)
        .map_or_else(
            || {
                ReviewCodexOutcome::Failed(
                    "Codex returned invalid or unknown hunk references".to_owned(),
                )
            },
            ReviewCodexOutcome::Answered,
        )
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt as _, path::PathBuf};

    use diffo_core::{ChangeKind, FileDiff, FileState, RepositorySnapshot};

    use super::*;

    fn request() -> AiCommitRequest {
        let snapshot = RepositorySnapshot {
            files: vec![FileState {
                path: PathBuf::from("src/main.rs"),
                old_path: None,
                kind: ChangeKind::Modified,
                staged: Some(FileDiff {
                    text: "STAGED_SENTINEL".to_owned(),
                }),
                unstaged: Some(FileDiff {
                    text: "UNSTAGED_SENTINEL".to_owned(),
                }),
            }],
            ..RepositorySnapshot::default()
        };
        AiCommitRequest::from_snapshot(&snapshot).expect("AI request")
    }

    #[test]
    fn accepts_only_the_strict_commit_response() {
        assert_eq!(
            parse_response(br#"{"subject":"feat: add AI commits"}"#),
            AiCommitOutcome::Generated("feat: add AI commits".to_owned())
        );
        assert!(matches!(
            parse_response(br#"{"subject":"bad\nsubject"}"#),
            AiCommitOutcome::Failed(_)
        ));
        assert!(matches!(
            parse_response(br#"{"subject":"message","body":"surprise"}"#),
            AiCommitOutcome::Failed(_)
        ));
    }

    #[test]
    fn rejects_subjects_over_seventy_two_characters() {
        let response = format!(r#"{{"subject":"{}"}}"#, "x".repeat(73));
        assert!(matches!(
            parse_response(response.as_bytes()),
            AiCommitOutcome::Failed(_)
        ));
    }

    #[test]
    fn invokes_codex_with_configured_model_read_only_schema_and_staged_stdin() {
        let repository = tempfile::tempdir().expect("repository directory");
        let executable = repository.path().join("fake-codex");
        let expected_cwd = repository.path().display().to_string();
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\n\
                 [ \"$1\" = exec ] || exit 11\n\
                 [ \"$2\" = --ephemeral ] || exit 12\n\
                 [ \"$3\" = --model ] || exit 13\n\
                 [ \"$4\" = {model} ] || exit 14\n\
                 [ \"$5\" = --sandbox ] || exit 15\n\
                 [ \"$6\" = read-only ] || exit 16\n\
                 [ \"$7\" = --output-schema ] || exit 17\n\
                 [ -f \"$8\" ] || exit 18\n\
                 [ \"$(pwd)\" = '{cwd}' ] || exit 19\n\
                 input=$(cat)\n\
                 case \"$input\" in *STAGED_SENTINEL*) ;; *) exit 20 ;; esac\n\
                 case \"$input\" in *UNSTAGED_SENTINEL*) exit 21 ;; esac\n\
                 printf '%s' '{{\"subject\":\"feat: generate commits\"}}'\n",
                model = AI_COMMIT_MODEL,
                cwd = expected_cwd.replace('\'', "'\\''")
            ),
        )
        .expect("write fake Codex");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("make fake Codex executable");

        let outcome = run_codex(
            executable.as_os_str(),
            repository.path(),
            &request(),
            &CancellationHandle::default(),
        );

        assert_eq!(
            outcome,
            AiCommitOutcome::Generated("feat: generate commits".to_owned())
        );
    }

    #[test]
    fn resolves_codex_from_inherited_or_login_shell_path() {
        let directory = tempfile::tempdir().expect("executable directory");
        let executable = directory.path().join("codex");
        fs::write(&executable, "#!/bin/sh\n").expect("write fake Codex");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("make fake Codex executable");

        assert_eq!(
            resolve_executable(
                "codex",
                Some(directory.path().as_os_str()),
                OsStr::new("/missing-shell")
            ),
            Some(executable.as_os_str().to_owned())
        );

        let shell = directory.path().join("login-shell");
        fs::write(
            &shell,
            format!(
                "#!/bin/sh\n\
                 [ \"$1\" = -lc ] || exit 31\n\
                 [ \"$2\" = 'command -v -- codex' ] || exit 32\n\
                 printf '%s\\n' '{}'\n",
                executable.display()
            ),
        )
        .expect("write fake login shell");
        fs::set_permissions(&shell, fs::Permissions::from_mode(0o755))
            .expect("make fake shell executable");

        assert_eq!(
            resolve_executable("codex", Some(OsStr::new("")), shell.as_os_str()),
            Some(executable.into_os_string())
        );
    }

    #[test]
    fn cancelled_request_does_not_start_the_executable() {
        let cancellation = CancellationHandle::default();
        cancellation.cancel();

        assert_eq!(
            run_codex(
                OsStr::new("definitely-not-a-real-codex"),
                Path::new("."),
                &request(),
                &cancellation,
            ),
            AiCommitOutcome::Cancelled
        );
    }
}
