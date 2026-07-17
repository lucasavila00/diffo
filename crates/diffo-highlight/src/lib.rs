use std::{collections::BTreeMap, path::Path};

use diffo_diff::{DiffBlock, DiffDocument, DiffLine};
use two_face::{
    re_exports::syntect::{
        easy::HighlightLines,
        highlighting::{FontStyle, Theme},
        parsing::{SyntaxReference, SyntaxSet},
    },
    theme::EmbeddedThemeName,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StyledSpan {
    pub text: String,
    pub foreground: Rgb,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HighlightedLine {
    pub spans: Vec<StyledSpan>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HighlightedDiff {
    pub old: BTreeMap<u32, HighlightedLine>,
    pub new: BTreeMap<u32, HighlightedLine>,
}

pub struct SyntaxHighlighter {
    syntaxes: SyntaxSet,
    theme: Theme,
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxHighlighter {
    #[must_use]
    pub fn new() -> Self {
        let themes = two_face::theme::extra();
        Self {
            syntaxes: two_face::syntax::extra_no_newlines(),
            theme: themes.get(EmbeddedThemeName::MonokaiExtended).clone(),
        }
    }

    #[must_use]
    pub fn highlight(&self, path: &Path, document: &DiffDocument) -> HighlightedDiff {
        let Some(syntax) = self.syntax_for(path, document) else {
            return HighlightedDiff::default();
        };
        let mut highlighted = HighlightedDiff::default();
        for hunk in &document.hunks {
            let mut old = Vec::new();
            let mut new = Vec::new();
            for block in &hunk.blocks {
                match block {
                    DiffBlock::Context(lines) => {
                        old.extend(lines.iter().filter(|line| line.old_number.is_some()));
                        new.extend(lines.iter().filter(|line| line.new_number.is_some()));
                    }
                    DiffBlock::Change { removed, added, .. } => {
                        old.extend(removed);
                        new.extend(added);
                    }
                    DiffBlock::Meta(_) => {}
                }
            }
            highlight_side(
                &self.syntaxes,
                syntax,
                &self.theme,
                old,
                |line| line.old_number,
                &mut highlighted.old,
            );
            highlight_side(
                &self.syntaxes,
                syntax,
                &self.theme,
                new,
                |line| line.new_number,
                &mut highlighted.new,
            );
        }
        highlighted
    }

    fn syntax_for<'a>(
        &'a self,
        path: &Path,
        document: &DiffDocument,
    ) -> Option<&'a SyntaxReference> {
        path.file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| self.syntaxes.find_syntax_by_extension(name))
            .or_else(|| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .and_then(|extension| self.syntaxes.find_syntax_by_extension(extension))
            })
            .or_else(|| {
                first_code_line(document)
                    .and_then(|line| self.syntaxes.find_syntax_by_first_line(line))
            })
    }
}

fn first_code_line(document: &DiffDocument) -> Option<&str> {
    document
        .hunks
        .iter()
        .flat_map(|hunk| &hunk.blocks)
        .find_map(|block| match block {
            DiffBlock::Context(lines) => lines.first().map(|line| line.text.as_str()),
            DiffBlock::Change { removed, added, .. } => removed
                .first()
                .or_else(|| added.first())
                .map(|line| line.text.as_str()),
            DiffBlock::Meta(_) => None,
        })
}

fn highlight_side(
    syntaxes: &SyntaxSet,
    syntax: &SyntaxReference,
    theme: &Theme,
    lines: Vec<&DiffLine>,
    number: impl Fn(&DiffLine) -> Option<u32>,
    output: &mut BTreeMap<u32, HighlightedLine>,
) {
    let mut highlighter = HighlightLines::new(syntax, theme);
    for line in lines {
        let Some(number) = number(line) else {
            continue;
        };
        let Ok(spans) = highlighter.highlight_line(&line.text, syntaxes) else {
            continue;
        };
        output.insert(
            number,
            HighlightedLine {
                spans: spans
                    .into_iter()
                    .map(|(style, text)| StyledSpan {
                        text: text.to_owned(),
                        foreground: Rgb {
                            red: style.foreground.r,
                            green: style.foreground.g,
                            blue: style.foreground.b,
                        },
                        bold: style.font_style.contains(FontStyle::BOLD),
                        italic: style.font_style.contains(FontStyle::ITALIC),
                        underline: style.font_style.contains(FontStyle::UNDERLINE),
                    })
                    .collect(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
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
                    channels.iter().max().expect("channel")
                        - channels.iter().min().expect("channel")
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
}
