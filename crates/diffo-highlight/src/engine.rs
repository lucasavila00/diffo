use std::{collections::BTreeMap, path::Path, thread};

use diffo_diff::{DiffBlock, DiffDocument, DiffLine};
use two_face::{
    re_exports::syntect::{
        easy::HighlightLines,
        highlighting::{FontStyle, Theme},
        parsing::{SyntaxReference, SyntaxSet},
    },
    theme::EmbeddedThemeName,
};

use crate::{
    HighlightWindowRequest, HighlightedDiff, HighlightedLine, HighlightedWindow, LineRange, Rgb,
    StyledSpan,
};

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
        self.highlight_window(
            path,
            document,
            HighlightWindowRequest {
                old: Some(LineRange::new(1, u32::MAX)),
                new: Some(LineRange::new(1, u32::MAX)),
                lookbehind_lines: 0,
                maximum_bytes_per_side: usize::MAX,
            },
        )
        .styles
    }

    #[must_use]
    pub fn highlight_window(
        &self,
        path: &Path,
        document: &DiffDocument,
        request: HighlightWindowRequest,
    ) -> HighlightedWindow {
        let Some(syntax) = self.syntax_for(path, document) else {
            return HighlightedWindow {
                old_coverage: request.old,
                new_coverage: request.new,
                ..HighlightedWindow::default()
            };
        };
        let old = collect_side(document, |line| line.old_number);
        let new = collect_side(document, |line| line.new_number);
        let ((old_styles, old_lines_processed), (new_styles, new_lines_processed)) =
            thread::scope(|scope| {
                let old_task = scope.spawn(|| {
                    highlight_side_window(
                        &self.syntaxes,
                        syntax,
                        &self.theme,
                        &old,
                        request.old,
                        request.lookbehind_lines,
                        request.maximum_bytes_per_side,
                    )
                });
                let new_task = scope.spawn(|| {
                    highlight_side_window(
                        &self.syntaxes,
                        syntax,
                        &self.theme,
                        &new,
                        request.new,
                        request.lookbehind_lines,
                        request.maximum_bytes_per_side,
                    )
                });
                (
                    old_task.join().unwrap_or_default(),
                    new_task.join().unwrap_or_default(),
                )
            });
        HighlightedWindow {
            styles: HighlightedDiff {
                old: old_styles,
                new: new_styles,
            },
            old_coverage: request.old,
            new_coverage: request.new,
            old_lines_processed,
            new_lines_processed,
        }
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

fn collect_side<'a>(
    document: &'a DiffDocument,
    number: impl Fn(&DiffLine) -> Option<u32>,
) -> Vec<(&'a DiffLine, u32)> {
    let mut side = Vec::new();
    let mut include = |line: &'a DiffLine| {
        if let Some(number) = number(line) {
            side.push((line, number));
        }
    };
    for block in document.hunks.iter().flat_map(|hunk| &hunk.blocks) {
        match block {
            DiffBlock::Context(lines) => lines.iter().for_each(&mut include),
            DiffBlock::Change { removed, added, .. } => {
                removed.iter().chain(added).for_each(&mut include);
            }
            DiffBlock::Meta(_) => {}
        }
    }
    side
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

fn highlight_side_window(
    syntaxes: &SyntaxSet,
    syntax: &SyntaxReference,
    theme: &Theme,
    lines: &[(&DiffLine, u32)],
    range: Option<LineRange>,
    lookbehind_lines: usize,
    maximum_bytes: usize,
) -> (BTreeMap<u32, HighlightedLine>, usize) {
    let Some(range) = range else {
        return (BTreeMap::new(), 0);
    };
    let start = range
        .start
        .saturating_sub(u32::try_from(lookbehind_lines).unwrap_or(u32::MAX));
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut output = BTreeMap::new();
    let mut bytes = 0_usize;
    let mut processed = 0_usize;
    for &(line, number) in lines {
        if number < start || number > range.end {
            continue;
        }
        let line_bytes = line.text.len();
        if bytes.saturating_add(line_bytes) > maximum_bytes {
            break;
        }
        bytes = bytes.saturating_add(line_bytes);
        processed = processed.saturating_add(1);
        let Ok(spans) = highlighter.highlight_line(&line.text, syntaxes) else {
            continue;
        };
        if !range.contains(number) {
            continue;
        }
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
    (output, processed)
}
