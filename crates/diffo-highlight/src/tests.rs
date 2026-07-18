use std::path::Path;

use diffo_diff::parse_unified_patch;

use super::SyntaxHighlighter;

#[test]
fn highlights_old_and_new_rust_lines() {
    let document = parse_unified_patch(
        "@@ -1,2 +1,2 @@\n fn main() {\n-    let old = true;\n+    let new = false;\n",
    )
    .expect("valid patch");
    let result = SyntaxHighlighter::new().highlight(Path::new("src/main.rs"), &document);

    assert!(result.old.get(&2).is_some_and(|line| line.spans.len() > 1));
    assert!(result.new.get(&2).is_some_and(|line| line.spans.len() > 1));
    assert!(
        result
            .new
            .get(&2)
            .expect("highlighted new line")
            .spans
            .iter()
            .any(|span| {
                let channels = [
                    span.foreground.red,
                    span.foreground.green,
                    span.foreground.blue,
                ];
                channels.iter().max().expect("channel") - channels.iter().min().expect("channel")
                    > 30
            }),
        "syntax palette should contain chromatic colors"
    );
}

#[test]
fn supports_bat_curated_syntaxes() {
    let highlighter = SyntaxHighlighter::new();
    for (path, code) in [
        ("app.tsx", "const view = <main>Hello</main>;"),
        ("script.py", "def hello(): return True"),
        ("Cargo.toml", "edition = \"2024\""),
        ("README.md", "# Heading"),
    ] {
        let patch = format!("@@ -0,0 +1 @@\n+{code}\n");
        let document = parse_unified_patch(&patch).expect("valid patch");
        let result = highlighter.highlight(Path::new(path), &document);
        assert!(!result.new[&1].spans.is_empty(), "{path} should highlight");
    }
}

#[test]
fn unknown_syntax_falls_back_to_plain_text() {
    let document = parse_unified_patch("@@ -0,0 +1 @@\n+some text\n").expect("valid patch");
    let result = SyntaxHighlighter::new().highlight(Path::new("file.unknown"), &document);

    assert!(result.old.is_empty());
    assert!(result.new.is_empty());
}
