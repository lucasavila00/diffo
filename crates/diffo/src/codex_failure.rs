use std::process::ExitStatus;

use diffo_ai_config::AI_MODEL;

const MAX_DETAIL_CHARS: usize = 500;

#[derive(Clone, Copy)]
enum KnownFailure {
    Authentication,
    RateLimit,
    ModelAccess,
    Access,
    IncompatibleCli,
    Configuration,
    Network,
    Service,
}

pub(super) fn process_failure(status: ExitStatus, stderr: &[u8], stderr_truncated: bool) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let normalized = stderr.to_ascii_lowercase();
    if let Some(failure) = known_failure(&normalized) {
        return known_failure_message(failure);
    }

    let summary = status_summary(status);
    if contains_sensitive_text(&normalized) {
        return format!("{summary}. Run Codex directly for details.");
    }
    let detail = safe_last_line(&stderr);
    match detail {
        Some(detail) if stderr_truncated => format!("{summary}: {detail} …"),
        Some(detail) => format!("{summary}: {detail}"),
        None => summary,
    }
}

fn known_failure(text: &str) -> Option<KnownFailure> {
    let groups = [
        (
            KnownFailure::Authentication,
            &[
                "not logged in",
                "login required",
                "authentication required",
                "authentication failed",
                "authentication not set up",
                "unauthorized",
                "invalid api key",
                "incorrect api key",
                "token expired",
                "token has expired",
                "refresh token",
                "status 401",
                "http 401",
            ][..],
        ),
        (
            KnownFailure::RateLimit,
            &[
                "rate limit",
                "too many requests",
                "usage limit",
                "quota exceeded",
                "insufficient quota",
                "status 429",
                "http 429",
            ][..],
        ),
        (
            KnownFailure::ModelAccess,
            &[
                "model not found",
                "unsupported model",
                "unknown model",
                "model is not available",
                "does not have access to model",
            ][..],
        ),
        (
            KnownFailure::Access,
            &[
                "forbidden",
                "access denied",
                "does not have access",
                "status 403",
                "http 403",
            ][..],
        ),
        (
            KnownFailure::IncompatibleCli,
            &[
                "unexpected argument",
                "unrecognized option",
                "unknown option",
                "invalid value for",
            ][..],
        ),
        (
            KnownFailure::Configuration,
            &[
                "required mcp",
                "mcp server",
                "failed to load config",
                "configuration error",
                "invalid configuration",
            ][..],
        ),
    ];
    groups
        .into_iter()
        .find_map(|(failure, needles)| contains_any(text, needles).then_some(failure))
        .or_else(|| remote_failure(text))
}

fn remote_failure(text: &str) -> Option<KnownFailure> {
    let network = [
        "could not resolve host",
        "connection refused",
        "connection reset",
        "network is unreachable",
        "dns error",
        "tls error",
        "certificate error",
        "request timed out",
        "connection timed out",
    ];
    let service = [
        "internal server error",
        "service unavailable",
        "bad gateway",
        "gateway timeout",
        "status 500",
        "status 502",
        "status 503",
        "status 504",
    ];
    if contains_any(text, &network) {
        Some(KnownFailure::Network)
    } else if contains_any(text, &service) {
        Some(KnownFailure::Service)
    } else {
        None
    }
}

fn known_failure_message(failure: KnownFailure) -> String {
    match failure {
        KnownFailure::Authentication => {
            "Codex authentication failed. Run `codex login`, then try again.".to_owned()
        }
        KnownFailure::RateLimit => "Codex usage is currently limited. Wait for the limit to reset or check the account's usage and billing, then try again.".to_owned(),
        KnownFailure::ModelAccess => format!("The Codex account cannot use {AI_MODEL}. Check workspace and model access, then try again."),
        KnownFailure::Access => "Codex access was denied. Check the account's workspace permissions, then try again.".to_owned(),
        KnownFailure::IncompatibleCli => "The installed Codex CLI is incompatible with Diffo. Update Codex, then restart Diffo.".to_owned(),
        KnownFailure::Configuration => "Codex configuration failed. Run Codex directly to repair its configuration, then try again.".to_owned(),
        KnownFailure::Network => "Codex could not reach OpenAI. Check the network connection, then try again.".to_owned(),
        KnownFailure::Service => {
            "The Codex service is temporarily unavailable. Try again shortly.".to_owned()
        }
    }
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn contains_sensitive_text(text: &str) -> bool {
    contains_any(
        text,
        &[
            "api key",
            "api_key",
            "authorization",
            "bearer ",
            "access token",
            "access_token",
            "refresh token",
            "refresh_token",
            "token=",
            "token:",
            "sk-",
        ],
    )
}

fn safe_last_line(stderr: &str) -> Option<String> {
    let line = stderr.lines().rev().find(|line| !line.trim().is_empty())?;
    let mut detail = String::new();
    for character in line.trim().chars() {
        if detail.chars().count() >= MAX_DETAIL_CHARS {
            detail.push('…');
            break;
        }
        if character.is_control() {
            detail.push(' ');
        } else {
            detail.push(character);
        }
    }
    let detail = detail.split_whitespace().collect::<Vec<_>>().join(" ");
    (!detail.is_empty()).then_some(detail)
}

fn status_summary(status: ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("Codex CLI failed with exit code {code}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt as _;

        if let Some(signal) = status.signal() {
            return format!("Codex CLI crashed after receiving signal {signal}");
        }
    }
    "Codex CLI terminated unexpectedly".to_owned()
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    fn failed_status() -> ExitStatus {
        Command::new("sh")
            .args(["-c", "exit 17"])
            .status()
            .expect("failed status")
    }

    #[test]
    fn classifies_actionable_codex_failures() {
        let cases = [
            ("client authentication not set up", "codex login"),
            (
                "429 Too Many Requests: rate limit reached",
                "usage is currently limited",
            ),
            ("model not found", AI_MODEL),
            ("403 Forbidden", "workspace permissions"),
            (
                "error: unexpected argument '--output-schema'",
                "Update Codex",
            ),
            (
                "required MCP server failed to initialize",
                "configuration failed",
            ),
            ("connection refused", "network connection"),
            ("503 Service Unavailable", "temporarily unavailable"),
        ];
        for (stderr, expected) in cases {
            assert!(
                process_failure(failed_status(), stderr.as_bytes(), false).contains(expected),
                "{stderr:?} did not produce {expected:?}"
            );
        }
    }

    #[test]
    fn unknown_failures_keep_a_bounded_safe_last_line() {
        let message = process_failure(
            failed_status(),
            format!("progress\n{}\nlast useful line", "x".repeat(600)).as_bytes(),
            true,
        );
        assert!(message.contains("exit code 17"));
        assert!(message.contains("last useful line"));
        assert!(message.ends_with('…'));
        assert!(message.chars().count() < 600);
    }

    #[test]
    fn sensitive_unknown_failures_never_echo_the_diagnostic() {
        let secret = "sk-secret-sentinel";
        let message = process_failure(
            failed_status(),
            format!("custom failure api_key={secret}").as_bytes(),
            false,
        );
        assert!(!message.contains(secret));
        assert!(message.contains("Run Codex directly"));
    }

    #[cfg(unix)]
    #[test]
    fn reports_signal_termination_as_a_crash() {
        let status = Command::new("sh")
            .args(["-c", "kill -SEGV $$"])
            .status()
            .expect("crashed status");
        assert!(process_failure(status, b"", false).contains("crashed"));
    }
}
