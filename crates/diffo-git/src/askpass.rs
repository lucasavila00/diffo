use std::{
    env, fs,
    io::{self, Read, Write},
    os::unix::{
        fs::PermissionsExt as _,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{Context as _, Result};
use diffo_core::{
    CancellationHandle, GitPrompt, PromptAnswer, PromptId, RepositoryOperationContext, SecretKind,
};

pub const ASKPASS_MARKER: &str = "DIFFO_INTERNAL_ASKPASS";
pub const ASKPASS_SOCKET: &str = "DIFFO_INTERNAL_ASKPASS_SOCKET";

const MAX_FIELD_BYTES: usize = 4_096;

pub struct AskpassBridge {
    _directory: tempfile::TempDir,
    socket: PathBuf,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
}

impl AskpassBridge {
    pub fn start(context: &RepositoryOperationContext) -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("diffo-askpass-")
            .tempdir()
            .context("failed to create askpass directory")?;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))
            .context("failed to protect askpass directory")?;
        let socket = directory.path().join("prompt.sock");
        let listener = UnixListener::bind(&socket).context("failed to bind askpass socket")?;
        listener
            .set_nonblocking(true)
            .context("failed to configure askpass socket")?;
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = Arc::clone(&stop);
        let prompts = Arc::clone(&context.prompts);
        let cancellation = context.cancellation.clone();
        let server = thread::Builder::new()
            .name("diffo-askpass".to_owned())
            .spawn(move || {
                let mut next_id = 1_u64;
                while !server_stop.load(Ordering::Acquire) && !cancellation.is_cancelled() {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            handle_connection(
                                stream,
                                PromptId(next_id),
                                Arc::clone(&prompts),
                                &cancellation,
                            );
                            next_id = next_id.saturating_add(1);
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
            })
            .context("failed to start askpass bridge")?;
        Ok(Self {
            _directory: directory,
            socket,
            stop,
            server: Some(server),
        })
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }
}

impl Drop for AskpassBridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

fn handle_connection(
    mut stream: UnixStream,
    id: PromptId,
    prompts: Arc<dyn diffo_core::PromptHandler>,
    cancellation: &CancellationHandle,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let Ok(prompt) = read_prompt(&mut stream) else {
        let _ = write_cancel(&mut stream);
        return;
    };
    let expected = PromptClass::of(&prompt);
    let handler_cancellation = cancellation.clone();
    let (answer_tx, answer_rx) = std::sync::mpsc::sync_channel(1);
    let handler = thread::spawn(move || {
        let answer = prompts.prompt(id, prompt, &handler_cancellation);
        let _ = answer_tx.send(answer);
    });
    let _ = stream.set_nonblocking(true);
    let answer = loop {
        match answer_rx.recv_timeout(Duration::from_millis(20)) {
            Ok(answer) => break answer,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let disconnected = match stream.read(&mut [0]) {
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => false,
                    Ok(_) | Err(_) => true,
                };
                if disconnected {
                    cancellation.cancel();
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                cancellation.cancel();
                let _ = handler.join();
                let _ = stream.set_nonblocking(false);
                let _ = write_cancel(&mut stream);
                return;
            }
        }
    };
    let _ = handler.join();
    let _ = stream.set_nonblocking(false);
    if cancellation.is_cancelled() {
        let _ = write_cancel(&mut stream);
        return;
    }
    let result = match (expected, answer) {
        (PromptClass::Text, PromptAnswer::Text(answer)) if valid_answer(&answer) => {
            write_answer(&mut stream, &answer)
        }
        (PromptClass::Confirm, PromptAnswer::Confirm) => write_answer(&mut stream, "yes"),
        _ => write_cancel(&mut stream),
    };
    let _ = result;
}

#[derive(Clone, Copy)]
enum PromptClass {
    Text,
    Confirm,
}

impl PromptClass {
    fn of(prompt: &GitPrompt) -> Self {
        match prompt {
            GitPrompt::Username { .. } | GitPrompt::Secret { .. } => Self::Text,
            GitPrompt::ConfirmSshHost { .. } => Self::Confirm,
        }
    }
}

fn valid_answer(answer: &str) -> bool {
    !answer.is_empty() && answer.len() <= MAX_FIELD_BYTES && !answer.chars().any(char::is_control)
}

fn write_cancel(stream: &mut UnixStream) -> io::Result<()> {
    stream.write_all(&[1])
}

fn write_answer(stream: &mut UnixStream, answer: &str) -> io::Result<()> {
    stream.write_all(&[0])?;
    write_field(stream, answer)
}

fn write_prompt(stream: &mut UnixStream, prompt: &GitPrompt) -> io::Result<()> {
    match prompt {
        GitPrompt::Username { host } => {
            stream.write_all(&[1])?;
            write_field(stream, host)
        }
        GitPrompt::Secret { kind, context } => {
            let kind = match kind {
                SecretKind::HttpsSecret => 1,
                SecretKind::SshKeyPassphrase => 2,
            };
            stream.write_all(&[2, kind])?;
            write_field(stream, context)
        }
        GitPrompt::ConfirmSshHost { host, fingerprint } => {
            stream.write_all(&[3])?;
            write_field(stream, host)?;
            write_field(stream, fingerprint)
        }
    }
}

fn read_prompt(stream: &mut UnixStream) -> io::Result<GitPrompt> {
    let mut kind = [0];
    stream.read_exact(&mut kind)?;
    match kind[0] {
        1 => Ok(GitPrompt::Username {
            host: read_display_field(stream)?,
        }),
        2 => {
            stream.read_exact(&mut kind)?;
            let secret = match kind[0] {
                1 => SecretKind::HttpsSecret,
                2 => SecretKind::SshKeyPassphrase,
                _ => return Err(io::Error::from(io::ErrorKind::InvalidData)),
            };
            Ok(GitPrompt::Secret {
                kind: secret,
                context: read_display_field(stream)?,
            })
        }
        3 => Ok(GitPrompt::ConfirmSshHost {
            host: read_display_field(stream)?,
            fingerprint: read_display_field(stream)?,
        }),
        _ => Err(io::Error::from(io::ErrorKind::InvalidData)),
    }
}

fn write_field(stream: &mut UnixStream, value: &str) -> io::Result<()> {
    let length =
        u32::try_from(value.len()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    stream.write_all(&length.to_be_bytes())?;
    stream.write_all(value.as_bytes())
}

fn read_field(stream: &mut UnixStream) -> io::Result<String> {
    let mut length = [0; 4];
    stream.read_exact(&mut length)?;
    let length = usize::try_from(u32::from_be_bytes(length))
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    if length == 0 || length > MAX_FIELD_BYTES {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))
}

fn read_display_field(stream: &mut UnixStream) -> io::Result<String> {
    let field = read_field(stream)?;
    if valid_display_field(&field) {
        Ok(field)
    } else {
        Err(io::Error::from(io::ErrorKind::InvalidData))
    }
}

fn valid_display_field(field: &str) -> bool {
    !field.chars().any(char::is_control)
}

/// Run the private askpass startup path when its internal marker is present.
///
/// Returns `None` during normal startup and an exit status for askpass startup.
#[must_use]
pub fn run_askpass_if_requested() -> Option<i32> {
    (env::var_os(ASKPASS_MARKER).is_some()).then(run_askpass)
}

fn run_askpass() -> i32 {
    match askpass_answer() {
        Ok(answer) => {
            let mut stdout = io::stdout().lock();
            i32::from(
                !(stdout.write_all(answer.as_bytes()).is_ok()
                    && stdout.write_all(b"\n").is_ok()
                    && stdout.flush().is_ok()),
            )
        }
        Err(()) => 1,
    }
}

fn askpass_answer() -> std::result::Result<String, ()> {
    let raw = env::args().nth(1).ok_or(())?;
    if env::args().nth(2).is_some() {
        return Err(());
    }
    let prompt_kind = env::var("SSH_ASKPASS_PROMPT").ok();
    let prompt = parse_prompt(&raw, prompt_kind.as_deref()).ok_or(())?;
    let socket = env::var_os(ASKPASS_SOCKET).ok_or(())?;
    let mut stream = UnixStream::connect(socket).map_err(|_| ())?;
    write_prompt(&mut stream, &prompt).map_err(|_| ())?;
    let mut status = [0];
    stream.read_exact(&mut status).map_err(|_| ())?;
    if status[0] != 0 {
        return Err(());
    }
    let answer = read_field(&mut stream).map_err(|_| ())?;
    valid_answer(&answer).then_some(answer).ok_or(())
}

fn parse_prompt(raw: &str, ssh_prompt: Option<&str>) -> Option<GitPrompt> {
    if raw.len() > MAX_FIELD_BYTES {
        return None;
    }
    match ssh_prompt {
        Some("confirm") => {
            return parse_host_confirmation_prompt(raw);
        }
        None if raw.contains('\n') => return parse_host_confirmation_prompt(raw),
        Some("none") | None => {}
        Some(_) => return None,
    }
    if raw.chars().any(char::is_control) {
        return None;
    }
    parse_quoted(raw, "Username for '", "': ")
        .and_then(|target| http_host_from_target(target).map(|host| GitPrompt::Username { host }))
        .or_else(|| {
            parse_quoted(raw, "Password for '", "': ").and_then(|target| {
                http_host_from_target(target).map(|context| GitPrompt::Secret {
                    kind: SecretKind::HttpsSecret,
                    context,
                })
            })
        })
        .or_else(|| {
            parse_quoted(raw, "Enter passphrase for key '", "': ").and_then(|context| {
                valid_display_field(context).then(|| GitPrompt::Secret {
                    kind: SecretKind::SshKeyPassphrase,
                    context: context.to_owned(),
                })
            })
        })
}

fn parse_quoted<'a>(raw: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let value = raw.strip_prefix(prefix)?.strip_suffix(suffix)?;
    (!value.is_empty() && !value.contains('\'')).then_some(value)
}

fn host_from_target(target: &str) -> Option<String> {
    let authority = target
        .split_once("://")
        .map_or(target, |(_, remainder)| remainder)
        .split('/')
        .next()?;
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if host.is_empty()
        || host.contains('@')
        || host.chars().any(char::is_whitespace)
        || !valid_display_field(host)
    {
        return None;
    }
    Some(host.to_owned())
}

fn http_host_from_target(target: &str) -> Option<String> {
    if !target.starts_with("https://") && !target.starts_with("http://") {
        return None;
    }
    host_from_target(target)
}

fn parse_host_confirmation(raw: &str) -> Option<GitPrompt> {
    let mut lines = raw.lines();
    let first = lines.next()?;
    let fingerprint_line = lines.next()?;
    let next = lines.next()?;
    let question = if next == "This key is not known by any other names." {
        lines.next()?
    } else {
        next
    };
    if lines.next().is_some()
        || question != "Are you sure you want to continue connecting (yes/no/[fingerprint])? "
    {
        return None;
    }
    let host = parse_quoted(
        first,
        "The authenticity of host '",
        "' can't be established.",
    )?;
    let host = host.split_once(" (").map_or(host, |(host, _)| host);
    if !valid_host(host) {
        return None;
    }
    let fingerprint_line = fingerprint_line
        .strip_suffix('.')
        .unwrap_or(fingerprint_line);
    let (algorithm, fingerprint) = fingerprint_line
        .split_once(" key fingerprint is ")
        .or_else(|| fingerprint_line.split_once(" key fingerprint is: "))?;
    if !matches!(algorithm, "ED25519" | "ECDSA" | "RSA") || !valid_fingerprint(fingerprint) {
        return None;
    }
    Some(GitPrompt::ConfirmSshHost {
        host: host.to_owned(),
        fingerprint: fingerprint.to_owned(),
    })
}

fn parse_host_confirmation_prompt(raw: &str) -> Option<GitPrompt> {
    if raw
        .chars()
        .any(|character| character.is_control() && character != '\n')
    {
        return None;
    }
    parse_host_confirmation(raw)
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 255
        && host
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_[]:".contains(character))
}

fn valid_fingerprint(fingerprint: &str) -> bool {
    let Some(value) = fingerprint.strip_prefix("SHA256:") else {
        return false;
    };
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "+/=".contains(character))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    struct Answer(Mutex<Option<PromptAnswer>>);

    impl diffo_core::PromptHandler for Answer {
        fn prompt(
            &self,
            _id: PromptId,
            _prompt: GitPrompt,
            _cancellation: &CancellationHandle,
        ) -> PromptAnswer {
            self.0
                .lock()
                .ok()
                .and_then(|mut answer| answer.take())
                .unwrap_or(PromptAnswer::Cancel)
        }
    }

    struct WaitForDisconnect;

    impl diffo_core::PromptHandler for WaitForDisconnect {
        fn prompt(
            &self,
            _id: PromptId,
            _prompt: GitPrompt,
            cancellation: &CancellationHandle,
        ) -> PromptAnswer {
            while !cancellation.is_cancelled() {
                thread::sleep(Duration::from_millis(5));
            }
            PromptAnswer::Cancel
        }
    }

    #[test]
    fn parses_supported_git_and_ssh_prompts() {
        assert_eq!(
            parse_prompt("Username for 'https://person@example.com': ", None),
            Some(GitPrompt::Username {
                host: "example.com".to_owned()
            })
        );
        assert_eq!(
            parse_prompt(
                "Password for 'https://person:sentinel@example.com/repo': ",
                None
            ),
            Some(GitPrompt::Secret {
                kind: SecretKind::HttpsSecret,
                context: "example.com".to_owned()
            })
        );
        assert_eq!(
            parse_prompt(
                "Enter passphrase for key '/keys/id_ed25519': ",
                Some("none")
            ),
            Some(GitPrompt::Secret {
                kind: SecretKind::SshKeyPassphrase,
                context: "/keys/id_ed25519".to_owned()
            })
        );
        let confirmation = concat!(
            "The authenticity of host 'git.example.com (192.0.2.1)' can't be established.\n",
            "ED25519 key fingerprint is SHA256:Abcdefghijklmnopqrstuvwxyz0123456789+/=.\n",
            "This key is not known by any other names.\n",
            "Are you sure you want to continue connecting (yes/no/[fingerprint])? "
        );
        assert_eq!(
            parse_prompt(confirmation, Some("confirm")),
            Some(GitPrompt::ConfirmSshHost {
                host: "git.example.com".to_owned(),
                fingerprint: "SHA256:Abcdefghijklmnopqrstuvwxyz0123456789+/=".to_owned()
            })
        );
        let loopback_confirmation = concat!(
            "The authenticity of host 'diffo-e2e ([127.0.0.1]:39397)' can't be established.\n",
            "ED25519 key fingerprint is SHA256:y4V5owxERf/fLbZfbkglknok7xY1IkZvRs+x9hOGGzE.\n",
            "This key is not known by any other names.\n",
            "Are you sure you want to continue connecting (yes/no/[fingerprint])? "
        );
        assert_eq!(
            parse_prompt(loopback_confirmation, Some("confirm")),
            Some(GitPrompt::ConfirmSshHost {
                host: "diffo-e2e".to_owned(),
                fingerprint: "SHA256:y4V5owxERf/fLbZfbkglknok7xY1IkZvRs+x9hOGGzE".to_owned()
            })
        );
        assert_eq!(
            parse_prompt(loopback_confirmation, None),
            Some(GitPrompt::ConfirmSshHost {
                host: "diffo-e2e".to_owned(),
                fingerprint: "SHA256:y4V5owxERf/fLbZfbkglknok7xY1IkZvRs+x9hOGGzE".to_owned()
            })
        );
    }

    #[test]
    fn rejects_unknown_malformed_and_control_bearing_prompts() {
        for prompt in [
            "Password: ",
            "Are you sure? yes/no",
            "Username for 'https://example.com':\u{1b}[31m ",
            "Enter PIN for key '/keys/id': ",
        ] {
            assert_eq!(parse_prompt(prompt, None), None, "accepted {prompt:?}");
        }
        assert_eq!(
            parse_prompt(
                "The authenticity of host 'example.com' can't be established.\nED25519 key fingerprint is missing.\nAre you sure you want to continue connecting (yes/no/[fingerprint])? ",
                Some("confirm")
            ),
            None
        );
        assert_eq!(parse_prompt("Password for 'git@example.com': ", None), None);
    }

    #[test]
    fn wire_protocol_round_trips_typed_prompts() {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let prompt = GitPrompt::ConfirmSshHost {
            host: "example.com".to_owned(),
            fingerprint: "SHA256:abc".to_owned(),
        };
        write_prompt(&mut client, &prompt).unwrap();
        assert_eq!(read_prompt(&mut server).unwrap(), prompt);
    }

    #[test]
    fn connection_writes_one_answer_or_cancels_without_output() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let handler = Arc::new(Answer(Mutex::new(Some(PromptAnswer::Confirm))));
        let cancellation = CancellationHandle::default();
        let task = {
            let handler = Arc::clone(&handler);
            let cancellation = cancellation.clone();
            thread::spawn(move || {
                handle_connection(server, PromptId(1), handler, &cancellation);
            })
        };
        write_prompt(
            &mut client,
            &GitPrompt::ConfirmSshHost {
                host: "example.com".to_owned(),
                fingerprint: "SHA256:abc".to_owned(),
            },
        )
        .unwrap();
        let mut status = [1];
        client.read_exact(&mut status).unwrap();
        assert_eq!(status, [0]);
        assert_eq!(read_field(&mut client).unwrap(), "yes");
        task.join().unwrap();

        let (mut client, server) = UnixStream::pair().unwrap();
        let task = thread::spawn(move || {
            handle_connection(
                server,
                PromptId(2),
                Arc::new(Answer(Mutex::new(Some(PromptAnswer::Cancel)))),
                &CancellationHandle::default(),
            );
        });
        write_prompt(
            &mut client,
            &GitPrompt::Username {
                host: "example.com".to_owned(),
            },
        )
        .unwrap();
        client.read_exact(&mut status).unwrap();
        assert_eq!(status, [1]);
        let mut extra = [0];
        assert_eq!(client.read(&mut extra).unwrap(), 0);
        task.join().unwrap();
    }

    #[test]
    fn helper_disconnect_cancels_the_pending_prompt() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let task = thread::spawn(move || {
            handle_connection(
                server,
                PromptId(1),
                Arc::new(WaitForDisconnect),
                &CancellationHandle::default(),
            );
        });
        write_prompt(
            &mut client,
            &GitPrompt::Username {
                host: "example.com".to_owned(),
            },
        )
        .unwrap();
        drop(client);
        task.join().unwrap();
    }
}
