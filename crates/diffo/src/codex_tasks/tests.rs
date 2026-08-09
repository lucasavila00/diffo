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

fn review_request() -> ReviewRequest {
    let snapshot = RepositorySnapshot {
        files: vec![FileState {
            path: PathBuf::from("src/main.rs"),
            old_path: None,
            kind: ChangeKind::Modified,
            staged: None,
            unstaged: Some(FileDiff {
                text: "@@ -1 +1 @@\n-old\n+new\n".to_owned(),
            }),
        }],
        ..RepositorySnapshot::default()
    };
    ReviewRequest::from_snapshot(&snapshot).expect("review request")
}

fn fake_codex(script: &str) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("Codex directory");
    let executable = directory.path().join("fake-codex");
    fs::write(&executable, format!("#!/bin/sh\n{script}\n")).expect("write fake Codex");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
        .expect("make fake Codex executable");
    (directory, executable)
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
fn accepts_only_a_strict_review_with_known_unique_targets() {
    let request = review_request();
    let context = request.prompt_context("repository");
    let target_id = context
        .split_once("<target id=\"")
        .and_then(|(_, value)| value.split_once('"'))
        .map(|(id, _)| id)
        .expect("target ID");
    let response = format!(
        r#"{{"overview":["Overview"],"stops":[{{"title":"Inspect behavior","category":"behavior","reason":"The behavior changes here.","target_id":"{target_id}"}}]}}"#
    );
    assert!(matches!(
        parse_review_response(&request, response.as_bytes()),
        ReviewCodexOutcome::Generated(_)
    ));

    let unknown_target = response.replace(target_id, "T0000000000000000");
    assert!(matches!(
        parse_review_response(&request, unknown_target.as_bytes()),
        ReviewCodexOutcome::Failed(_)
    ));
    let unknown_field = response.replacen(
        r#""overview":["Overview"]"#,
        r#""overview":["Overview"],"extra":true"#,
        1,
    );
    assert!(matches!(
        parse_review_response(&request, unknown_field.as_bytes()),
        ReviewCodexOutcome::Failed(_)
    ));
    let repeated_target = response.replace(
        "]}",
        &format!(
            r#",{{"title":"Inspect twice","category":"correctness","reason":"This repeats the same target.","target_id":"{target_id}"}}]}}"#
        ),
    );
    assert!(matches!(
        parse_review_response(&request, repeated_target.as_bytes()),
        ReviewCodexOutcome::Failed(_)
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
fn reports_empty_and_oversized_responses_explicitly() {
    assert!(matches!(
        parse_response(b" \n"),
        AiCommitOutcome::Failed(message) if message.contains("no commit message")
    ));
    assert!(matches!(
        finish_codex(&BoundedOutput {
            bytes: Vec::new(),
            truncated: true,
        }),
        RawCodexOutcome::Failed(message) if message.contains("oversized")
    ));
}

#[test]
fn bounded_stderr_keeps_the_latest_diagnostic() {
    let mut output = vec![b'x'; MAX_CODEX_OUTPUT_BYTES + 20];
    output.extend_from_slice(b"\nlatest diagnostic\n");
    let retained = read_in_background(io::Cursor::new(output), true)
        .join()
        .expect("reader thread")
        .expect("bounded output");
    assert!(retained.truncated);
    assert!(retained.bytes.ends_with(b"latest diagnostic\n"));
}

#[test]
fn invokes_codex_with_configured_model_read_only_schema_and_staged_stdin() {
    let repository = tempfile::tempdir().expect("repository directory");
    let expected_cwd = repository.path().display().to_string();
    let (_codex_directory, executable) = fake_codex(&format!(
        "[ \"$1\" = exec ] || exit 11\n\
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
         printf '%s' '{{\"subject\":\"feat: generate commits\"}}'",
        model = AI_COMMIT_MODEL,
        cwd = expected_cwd.replace('\'', "'\\''")
    ));

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
fn preserves_early_authentication_failure_instead_of_the_broken_pipe() {
    let (directory, executable) = fake_codex("echo '401 Unauthorized: token expired' >&2\nexit 1");
    let outcome = run_codex(
        executable.as_os_str(),
        directory.path(),
        &request(),
        &CancellationHandle::default(),
    );
    let AiCommitOutcome::Failed(message) = outcome else {
        panic!("expected failure");
    };
    assert!(message.contains("codex login"));
    assert!(!message.contains("token expired"));
}

#[cfg(unix)]
#[test]
fn reports_a_crashed_codex_process() {
    let (directory, executable) = fake_codex("kill -SEGV $$");
    let outcome = run_codex(
        executable.as_os_str(),
        directory.path(),
        &request(),
        &CancellationHandle::default(),
    );
    assert!(matches!(outcome, AiCommitOutcome::Failed(message) if message.contains("crashed")));
}

#[cfg(unix)]
#[test]
fn reports_a_codex_executable_without_execute_permission() {
    let directory = tempfile::tempdir().expect("Codex directory");
    let executable = directory.path().join("codex");
    fs::write(&executable, "#!/bin/sh\n").expect("write Codex");
    let outcome = run_codex(
        executable.as_os_str(),
        directory.path(),
        &request(),
        &CancellationHandle::default(),
    );
    assert!(
        matches!(outcome, AiCommitOutcome::Failed(message) if message.contains("not executable"))
    );
}

#[test]
fn bounds_a_codex_process_that_never_finishes() {
    let mut command = Command::new("sh");
    command.args(["-c", "while :; do :; done"]);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;

        command.process_group(0);
    }
    let mut child = command.spawn().expect("hanging child");
    let outcome = wait_for_child(
        &mut child,
        &CancellationHandle::default(),
        Duration::from_millis(40),
    );
    terminate_child(&mut child);
    assert!(matches!(outcome, WaitOutcome::TimedOut));
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
        Ok(Some(executable.as_os_str().to_owned()))
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
        Ok(Some(executable.into_os_string()))
    );
}

#[cfg(unix)]
#[test]
fn rejects_a_non_executable_codex_during_startup_resolution() {
    let directory = tempfile::tempdir().expect("executable directory");
    let executable = directory.path().join("codex");
    fs::write(&executable, "#!/bin/sh\n").expect("write fake Codex");

    assert!(
        resolve_executable(
            "codex",
            Some(directory.path().as_os_str()),
            OsStr::new("/missing-shell")
        )
        .unwrap_err()
        .contains("not executable")
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
