use std::{
    ffi::{OsStr, OsString},
    io::{self, Read, Write as _},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::{
        OnceLock,
        mpsc::{Receiver, Sender, channel},
    },
    thread,
    time::Duration,
};

#[cfg(any(not(feature = "codex-mock"), test))]
use std::env;

use diffo_ai_config::{
    AI_COMMIT_MODEL, AI_COMMIT_PROMPT, AI_COMMIT_SCHEMA, CODEX_EXECUTABLE, CODEX_SANDBOX,
    MAX_CODEX_OUTPUT_BYTES,
};
use diffo_app::workbench::{AiCommitOutcome, AiCommitRequest, Workbench};
use diffo_core::{ApplicationCommandId, CancellationHandle, FailureKind, OperationFailure};
use diffo_repository_service::RepositoryService;
use serde::Deserialize;
use tempfile::NamedTempFile;

static RESOLVED_CODEX_EXECUTABLE: OnceLock<OsString> = OnceLock::new();

enum CodexExecutable {
    Found(&'static OsStr),
    Missing(String),
}

pub(crate) struct CodexTasks {
    repository_root: PathBuf,
    sender: Sender<(ApplicationCommandId, AiCommitOutcome)>,
    receiver: Receiver<(ApplicationCommandId, AiCommitOutcome)>,
}

impl CodexTasks {
    pub(crate) fn new(repository_root: PathBuf) -> Self {
        let (sender, receiver) = channel();
        Self {
            repository_root,
            sender,
            receiver,
        }
    }

    pub(crate) fn start(
        &self,
        id: ApplicationCommandId,
        request: AiCommitRequest,
        cancellation: CancellationHandle,
    ) {
        let sender = self.sender.clone();
        let repository_root = self.repository_root.clone();
        thread::spawn(move || {
            let outcome = match selected_codex_executable() {
                CodexExecutable::Found(executable) => {
                    run_codex(executable, &repository_root, &request, &cancellation)
                }
                CodexExecutable::Missing(error) => AiCommitOutcome::Failed(error),
            };
            let _ = sender.send((id, outcome));
        });
    }

    pub(crate) fn drain(&self, workbench: &mut Workbench, repository_service: &RepositoryService) {
        while let Ok((id, outcome)) = self.receiver.try_recv() {
            let Some(handoff) = workbench.ai_commit_finished(id, outcome) else {
                continue;
            };
            if !repository_service.execute(id, handoff.action.clone(), handoff.cancellation) {
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
    }
}

fn selected_codex_executable() -> CodexExecutable {
    if let Some(executable) = RESOLVED_CODEX_EXECUTABLE.get() {
        return CodexExecutable::Found(executable);
    }

    #[cfg(feature = "codex-mock")]
    let resolved = Some(OsString::from(CODEX_EXECUTABLE));
    #[cfg(not(feature = "codex-mock"))]
    let resolved = resolve_production_codex();

    match resolved {
        Some(executable) => {
            let executable = RESOLVED_CODEX_EXECUTABLE.get_or_init(|| executable);
            CodexExecutable::Found(executable)
        }
        None => CodexExecutable::Missing(format!(
            "Codex CLI was not found in your shell PATH. Run `{CODEX_EXECUTABLE} --version` in the shell that starts Diffo, then press i again."
        )),
    }
}

#[cfg(not(feature = "codex-mock"))]
fn resolve_production_codex() -> Option<OsString> {
    let inherited_path = env::var_os("PATH");
    let shell = env::var_os("SHELL").unwrap_or_else(|| OsString::from("/bin/sh"));
    resolve_executable(CODEX_EXECUTABLE, inherited_path.as_deref(), &shell)
}

#[cfg(any(not(feature = "codex-mock"), test))]
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

#[cfg(any(not(feature = "codex-mock"), test))]
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
    if cancellation.is_cancelled() {
        return AiCommitOutcome::Cancelled;
    }
    let repository_name = repository_root
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("repository");
    let context = request.prompt_context(repository_name);
    let mut schema = match NamedTempFile::new() {
        Ok(schema) => schema,
        Err(error) => {
            return AiCommitOutcome::Failed(format!(
                "Could not create the Codex response schema: {error}"
            ));
        }
    };
    if let Err(error) = schema.write_all(AI_COMMIT_SCHEMA.as_bytes()) {
        return AiCommitOutcome::Failed(format!(
            "Could not write the Codex response schema: {error}"
        ));
    }

    let mut child = match Command::new(executable)
        .current_dir(repository_root)
        .args(["exec", "--ephemeral", "--model", AI_COMMIT_MODEL])
        .args(["--sandbox", CODEX_SANDBOX, "--output-schema"])
        .arg(schema.path())
        .arg(AI_COMMIT_PROMPT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return AiCommitOutcome::Failed(
                "The Codex CLI disappeared after Diffo found it. Run `codex --version` in your shell, then press i again."
                    .to_owned(),
            );
        }
        Err(error) => {
            return AiCommitOutcome::Failed(format!("Could not start Codex CLI: {error}"));
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
        return AiCommitOutcome::Failed(format!("Could not send staged changes to Codex: {error}"));
    }

    let status = loop {
        if cancellation.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = join_output(stdout);
            let _ = join_output(stderr);
            return AiCommitOutcome::Cancelled;
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = join_output(stdout);
                let _ = join_output(stderr);
                return AiCommitOutcome::Failed(format!("Could not wait for Codex CLI: {error}"));
            }
        }
    };

    let stdout = match join_output(stdout) {
        Ok(output) => output,
        Err(error) => {
            return AiCommitOutcome::Failed(format!("Could not read the Codex response: {error}"));
        }
    };
    let stderr = match join_output(stderr) {
        Ok(output) => output,
        Err(error) => {
            return AiCommitOutcome::Failed(format!("Could not read Codex progress: {error}"));
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
) -> AiCommitOutcome {
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr.bytes).trim().to_owned();
        return AiCommitOutcome::Failed(if detail.is_empty() {
            format!("Codex CLI exited with {status}")
        } else if stderr.truncated {
            format!("{detail}\n…")
        } else {
            detail
        });
    }
    if stdout.truncated {
        return AiCommitOutcome::Failed("Codex returned an oversized response".to_owned());
    }
    parse_response(&stdout.bytes)
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
