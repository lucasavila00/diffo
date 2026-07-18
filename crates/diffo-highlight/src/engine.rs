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

use crate::{HighlightedDiff, HighlightedLine, Rgb, StyledSpan};

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
