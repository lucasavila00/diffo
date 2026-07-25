use std::path::Path;

use diffo_diff::parse_unified_patch;

use super::{
    HIGHLIGHT_LOOKBEHIND_LINES, HighlightWindowRequest, HighlightedLine, LineRange,
    SyntaxHighlighter,
};

fn compact_line(line: &HighlightedLine) -> String {
    line.spans
        .iter()
        .map(|span| {
            format!(
                "{:?}=#{:02x}{:02x}{:02x}",
                span.text, span.foreground.red, span.foreground.green, span.foreground.blue
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

#[test]
fn highlights_old_and_new_rust_lines() {
    let document = parse_unified_patch(
        "@@ -1,2 +1,2 @@\n fn main() {\n-    let old = true;\n+    let new = false;\n",
    )
    .expect("valid patch");
    let result = SyntaxHighlighter::new().highlight(Path::new("src/main.rs"), &document);

    let compact = [
        (
            "old",
            result
                .old
                .iter()
                .map(|(number, line)| (*number, compact_line(line)))
                .collect::<Vec<_>>(),
        ),
        (
            "new",
            result
                .new
                .iter()
                .map(|(number, line)| (*number, compact_line(line)))
                .collect::<Vec<_>>(),
        ),
    ];
    insta::assert_debug_snapshot!(compact);
}

#[test]
fn supports_bat_curated_syntaxes() {
    let highlighter = SyntaxHighlighter::new();
    let mut cases = Vec::new();
    for (path, code) in [
        ("app.tsx", "const view = <main>Hello</main>;"),
        ("script.py", "def hello(): return True"),
        ("Cargo.toml", "edition = \"2024\""),
        ("README.md", "# Heading"),
    ] {
        let patch = format!("@@ -0,0 +1 @@\n+{code}\n");
        let document = parse_unified_patch(&patch).expect("valid patch");
        let result = highlighter.highlight(Path::new(path), &document);
        cases.push((path, compact_line(&result.new[&1])));
    }
    insta::assert_debug_snapshot!(cases);
}

#[test]
fn unknown_syntax_falls_back_to_plain_text() {
    let document = parse_unified_patch("@@ -0,0 +1 @@\n+some text\n").expect("valid patch");
    let result = SyntaxHighlighter::new().highlight(Path::new("file.unknown"), &document);

    assert!(result.old.is_empty());
    assert!(result.new.is_empty());
}

#[test]
fn window_highlighting_bounds_work_and_reports_coverage() {
    let mut patch = String::from("@@ -1,1000 +1,1000 @@\n");
    for line in 1..=1_000 {
        use std::fmt::Write as _;
        writeln!(patch, " pub const LINE_{line}: usize = {line};").unwrap();
    }
    let document = parse_unified_patch(&patch).expect("valid patch");
    let range = LineRange::new(900, 930);
    let result = SyntaxHighlighter::new().highlight_window(
        Path::new("src/main.rs"),
        &document,
        HighlightWindowRequest {
            old: Some(range),
            new: Some(range),
            lookbehind_lines: 64,
            maximum_bytes_per_side: usize::MAX,
        },
    );

    assert_eq!(result.old_coverage, Some(range));
    assert_eq!(result.new_coverage, Some(range));
    assert_eq!(result.old_lines_processed, 95);
    assert_eq!(result.new_lines_processed, 95);
    assert!(result.styles.old.contains_key(&900));
    assert!(result.styles.new.contains_key(&930));
    assert!(!result.styles.new.contains_key(&899));
}

#[test]
fn window_byte_budget_finishes_with_plain_fallback() {
    let document = parse_unified_patch(
        "@@ -0,0 +1,2 @@\n+pub const FIRST: usize = 1;\n+pub const SECOND: usize = 2;\n",
    )
    .expect("valid patch");
    let range = LineRange::new(1, 2);
    let result = SyntaxHighlighter::new().highlight_window(
        Path::new("src/main.rs"),
        &document,
        HighlightWindowRequest {
            old: None,
            new: Some(range),
            lookbehind_lines: 0,
            maximum_bytes_per_side: 1,
        },
    );

    assert_eq!(result.new_coverage, Some(range));
    assert_eq!(result.new_lines_processed, 0);
    assert!(result.styles.new.is_empty());
}

#[test]
fn window_lookbehind_preserves_a_multiline_construct() {
    use std::fmt::Write as _;

    let mut patch = String::from("@@ -1,300 +1,300 @@\n");
    for number in 1..=300 {
        let code = match number {
            100 => "/* comment begins",
            300 => "comment ends */",
            _ => "comment body",
        };
        writeln!(patch, " {code}").unwrap();
    }
    let document = parse_unified_patch(&patch).expect("valid patch");
    let highlighter = SyntaxHighlighter::new();
    let complete = highlighter.highlight(Path::new("src/main.rs"), &document);
    let range = LineRange::new(300, 300);
    let window = highlighter.highlight_window(
        Path::new("src/main.rs"),
        &document,
        HighlightWindowRequest {
            old: Some(range),
            new: Some(range),
            lookbehind_lines: HIGHLIGHT_LOOKBEHIND_LINES,
            maximum_bytes_per_side: usize::MAX,
        },
    );

    assert_eq!(window.styles.new[&300], complete.new[&300]);
}
