#![doc = include_str!("../README.md")]

use std::{env, fs, io::Read as _, path::Path};

use diffo_ai_config::{
    AI_COMMIT_MODEL, AI_COMMIT_PROMPT, AI_COMMIT_SCHEMA, AI_REVIEW_ASK_PROMPT,
    AI_REVIEW_ASK_SCHEMA, AI_REVIEW_MODEL, AI_REVIEW_PROMPT, AI_REVIEW_SCHEMA, CODEX_SANDBOX,
};

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
        || ![AI_COMMIT_MODEL, AI_REVIEW_MODEL].contains(&arguments[3].as_str())
        || arguments[4] != "--sandbox"
        || arguments[5] != CODEX_SANDBOX
        || arguments[6] != "--output-schema"
    {
        return Err(format!("unexpected invocation: {arguments:?}"));
    }
    let schema = fs::read_to_string(Path::new(&arguments[7]))
        .map_err(|error| format!("could not read output schema: {error}"))?;
    let mut context = String::new();
    std::io::stdin()
        .read_to_string(&mut context)
        .map_err(|error| format!("could not read prompt context: {error}"))?;
    match arguments[8].as_str() {
        AI_COMMIT_PROMPT => {
            if schema != AI_COMMIT_SCHEMA
                || !context.contains("<staged-changes ")
                || !context.contains("<staged-diff>")
            {
                return Err("commit request does not match the fixed AI policy".to_owned());
            }
            println!(r#"{{"subject":"{SUBJECT}"}}"#);
        }
        AI_REVIEW_PROMPT => {
            let hunk_id = first_hunk_id(&context);
            if schema != AI_REVIEW_SCHEMA
                || !context.contains("<changes total=")
                || hunk_id.is_none()
            {
                return Err("review request does not match the fixed AI policy".to_owned());
            }
            let hunk_id = hunk_id.expect("checked above");
            println!(
                r#"{{"overview":["The change updates the reviewed behavior."],"stops":[{{"title":"Inspect the main change","category":"behavior","reason":"This hunk contains the primary behavior change.","primary_hunk_id":"{hunk_id}","related_hunk_ids":[]}}]}}"#
            );
        }
        AI_REVIEW_ASK_PROMPT => {
            let hunk_id = first_hunk_id(&context);
            if schema != AI_REVIEW_ASK_SCHEMA
                || !context.contains("<review-map>")
                || !context.contains("<question>")
                || hunk_id.is_none()
            {
                return Err("ask request does not match the fixed AI policy".to_owned());
            }
            let hunk_id = hunk_id.expect("checked above");
            println!(
                r#"{{"text":["The main behavior change is in the linked hunk."],"hunk_ids":["{hunk_id}"]}}"#
            );
        }
        _ => return Err("prompt does not match the fixed AI policy".to_owned()),
    }
    Ok(())
}

fn first_hunk_id(context: &str) -> Option<&str> {
    let value = context.split_once("<hunk id=\"")?.1;
    value.split_once('"').map(|(id, _)| id).filter(|id| {
        id.starts_with('H')
            && id.len() > 1
            && id[1..]
                .chars()
                .all(|character| character.is_ascii_hexdigit())
    })
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
