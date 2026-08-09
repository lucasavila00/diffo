#![doc = include_str!("../README.md")]

use std::{env, fs, io::Read as _, path::Path};

use diffo_ai_config::{AI_COMMIT_MODEL, AI_COMMIT_PROMPT, AI_COMMIT_SCHEMA, CODEX_SANDBOX};

const SUBJECT: &str = "test: create commit with Codex";

fn main() {
    if let Err(error) = run(env::args().skip(1)) {
        eprintln!("codex-mock: {error}");
        std::process::exit(1);
    }
}

fn run(arguments: impl IntoIterator<Item = String>) -> Result<(), String> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    if arguments.len() != 9
        || arguments[0] != "exec"
        || arguments[1] != "--ephemeral"
        || arguments[2] != "--model"
        || arguments[3] != AI_COMMIT_MODEL
        || arguments[4] != "--sandbox"
        || arguments[5] != CODEX_SANDBOX
        || arguments[6] != "--output-schema"
    {
        return Err(format!("unexpected invocation: {arguments:?}"));
    }
    let schema = fs::read_to_string(Path::new(&arguments[7]))
        .map_err(|error| format!("could not read output schema: {error}"))?;
    if schema != AI_COMMIT_SCHEMA {
        return Err("output schema does not match the fixed AI policy".to_owned());
    }
    if arguments[8] != AI_COMMIT_PROMPT {
        return Err("prompt does not match the fixed AI policy".to_owned());
    }

    let mut context = String::new();
    std::io::stdin()
        .read_to_string(&mut context)
        .map_err(|error| format!("could not read prompt context: {error}"))?;
    if !context.contains("<staged-changes ") || !context.contains("<staged-diff>") {
        return Err("staged context is missing".to_owned());
    }

    println!(r#"{{"subject":"{SUBJECT}"}}"#);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_satisfies_the_production_contract() {
        assert!(!SUBJECT.is_empty());
        assert!(SUBJECT.chars().count() <= 72);
        assert!(!SUBJECT.chars().any(char::is_control));
    }

    #[test]
    fn rejects_wrong_or_extra_cli_arguments_before_reading_stdin() {
        let expected = [
            "exec",
            "--ephemeral",
            "--model",
            AI_COMMIT_MODEL,
            "--sandbox",
            "read-only",
            "--output-schema",
            "/missing-schema",
            "prompt",
        ];
        let mut wrong_model = expected.map(str::to_owned);
        wrong_model[3] = format!("not-{AI_COMMIT_MODEL}");
        assert!(
            run(wrong_model)
                .unwrap_err()
                .contains("unexpected invocation")
        );

        let mut extra = expected.map(str::to_owned).to_vec();
        extra.push("--json".to_owned());
        assert!(run(extra).unwrap_err().contains("unexpected invocation"));
    }
}
