use std::{path::PathBuf, sync::Arc};

use diffo_diff::{DiffBlock, DiffDocument, RowKind, parse_unified_patch};
use diffo_highlight::{
    HIGHLIGHT_LOOKBEHIND_LINES, HighlightWindowRequest, HighlightedDiff,
    MAX_HIGHLIGHT_BYTES_PER_SIDE, MAX_HIGHLIGHT_FILE_LINES, SyntaxHighlighter,
};
use diffo_ui::{terminal_safe_text, text_view::centered_window};
use ratatui::text::Line;

use crate::diff::raw_hunk_line;

#[derive(Clone, Debug)]
pub(super) struct PrepareRequest {
    pub(super) id: u64,
    pub(super) commit_id: String,
    pub(super) summary: String,
    pub(super) patch: Arc<str>,
    pub(super) target_scroll: usize,
    pub(super) viewport_rows: usize,
    pub(super) window_viewports: usize,
}

pub(super) struct PrepareOutcome {
    pub(super) id: u64,
    pub(super) prepared: PreparedPatch,
}

#[derive(Clone)]
pub(super) struct PreparedPatch {
    pub(super) commit_id: String,
    pub(super) summary: String,
    pub(super) patch: Arc<str>,
    pub(super) lines: Vec<Line<'static>>,
    pub(super) widths: Vec<usize>,
    pub(super) syntax_coverage: std::ops::Range<usize>,
    pub(super) target_scroll: usize,
}

impl PreparedPatch {
    pub(super) fn title(&self) -> Line<'static> {
        let short = self.commit_id.get(..7).unwrap_or(&self.commit_id);
        Line::raw(format!(" {short} · {} ", terminal_safe_text(&self.summary)))
    }

    pub(super) fn syntax_ready(&self, scroll: usize, viewport_rows: usize) -> bool {
        let end = scroll.saturating_add(viewport_rows).min(self.lines.len());
        scroll >= self.syntax_coverage.start && end <= self.syntax_coverage.end
    }
}

struct FileSection {
    path: PathBuf,
    document: DiffDocument,
}

struct PatchRow {
    file: Option<usize>,
    prefix: Option<char>,
    text: String,
    kind: RowKind,
    old_number: Option<u32>,
    new_number: Option<u32>,
}

pub(super) fn prepare(
    request: PrepareRequest,
    syntax_highlighter: &SyntaxHighlighter,
) -> PrepareOutcome {
    let (rows, sections) = patch_rows(&request.patch);
    let coverage = centered_window(
        request.target_scroll,
        rows.len(),
        request.viewport_rows,
        request.window_viewports,
    );
    let visible_files = rows
        .iter()
        .skip(coverage.start)
        .take(coverage.len())
        .filter_map(|row| row.file)
        .collect::<std::collections::BTreeSet<_>>();
    let byte_budget = MAX_HIGHLIGHT_BYTES_PER_SIDE / visible_files.len().max(1);
    let mut syntax_by_file = vec![HighlightedDiff::default(); sections.len()];
    for file in visible_files {
        let section = &sections[file];
        if file_line_count(&section.document) >= MAX_HIGHLIGHT_FILE_LINES {
            continue;
        }
        let mut old = None;
        let mut new = None;
        for row in rows
            .iter()
            .skip(coverage.start)
            .take(coverage.len())
            .filter(|row| row.file == Some(file))
        {
            include_line(&mut old, row.old_number);
            include_line(&mut new, row.new_number);
        }
        syntax_by_file[file] = syntax_highlighter
            .highlight_window(
                &section.path,
                &section.document,
                HighlightWindowRequest {
                    old,
                    new,
                    lookbehind_lines: HIGHLIGHT_LOOKBEHIND_LINES,
                    maximum_bytes_per_side: byte_budget,
                },
            )
            .styles;
    }
    let lines = rows
        .into_iter()
        .map(|row| {
            let syntax = row.file.and_then(|file| {
                let file_syntax = &syntax_by_file[file];
                match row.kind {
                    RowKind::Removed => row
                        .old_number
                        .and_then(|number| file_syntax.old.get(&number)),
                    RowKind::Added | RowKind::Context | RowKind::Changed => row
                        .new_number
                        .and_then(|number| file_syntax.new.get(&number)),
                    RowKind::Header | RowKind::Conflict | RowKind::Meta => None,
                }
            });
            raw_hunk_line(row.prefix, &row.text, row.kind, syntax)
        })
        .collect::<Vec<_>>();
    let widths = lines.iter().map(Line::width).collect();
    PrepareOutcome {
        id: request.id,
        prepared: PreparedPatch {
            commit_id: request.commit_id,
            summary: request.summary,
            patch: request.patch,
            lines,
            widths,
            syntax_coverage: coverage,
            target_scroll: request.target_scroll,
        },
    }
}

fn patch_rows(raw_patch: &str) -> (Vec<PatchRow>, Vec<FileSection>) {
    let mut raw_sections = Vec::<String>::new();
    let mut paths = Vec::<PathBuf>::new();
    let mut rows = Vec::new();
    let mut file = None;
    let mut old_line = 0_u32;
    let mut new_line = 0_u32;
    let mut in_hunk = false;

    for line in raw_patch.lines() {
        if line.starts_with("diff --git ") {
            file = Some(raw_sections.len());
            raw_sections.push(String::new());
            paths.push(PathBuf::new());
            in_hunk = false;
        }
        if let Some(file) = file {
            raw_sections[file].push_str(line);
            raw_sections[file].push('\n');
            if let Some(candidate) = patch_file_path(line, "+++ ", "b/") {
                paths[file] = candidate;
            } else if paths[file].as_os_str().is_empty()
                && let Some(candidate) = patch_file_path(line, "--- ", "a/")
            {
                paths[file] = candidate;
            }
        }

        let (prefix, kind, old_number, new_number) = if line.starts_with("@@ ") {
            if let Some((old, new)) = hunk_starts(line) {
                old_line = old;
                new_line = new;
                in_hunk = true;
            }
            (None, RowKind::Header, None, None)
        } else if in_hunk {
            if let Some(text) = line.strip_prefix(' ') {
                let numbers = (Some(old_line), Some(new_line));
                old_line = old_line.saturating_add(1);
                new_line = new_line.saturating_add(1);
                rows.push(PatchRow {
                    file,
                    prefix: Some(' '),
                    text: text.to_owned(),
                    kind: RowKind::Context,
                    old_number: numbers.0,
                    new_number: numbers.1,
                });
                continue;
            } else if let Some(text) = line.strip_prefix('-') {
                let number = old_line;
                old_line = old_line.saturating_add(1);
                rows.push(PatchRow {
                    file,
                    prefix: Some('-'),
                    text: text.to_owned(),
                    kind: RowKind::Removed,
                    old_number: Some(number),
                    new_number: None,
                });
                continue;
            } else if let Some(text) = line.strip_prefix('+') {
                let number = new_line;
                new_line = new_line.saturating_add(1);
                rows.push(PatchRow {
                    file,
                    prefix: Some('+'),
                    text: text.to_owned(),
                    kind: RowKind::Added,
                    old_number: None,
                    new_number: Some(number),
                });
                continue;
            } else if line.starts_with('\\') {
                (None, RowKind::Meta, None, None)
            } else {
                in_hunk = false;
                (None, RowKind::Meta, None, None)
            }
        } else {
            (None, RowKind::Meta, None, None)
        };
        rows.push(PatchRow {
            file,
            prefix,
            text: line.to_owned(),
            kind,
            old_number,
            new_number,
        });
    }

    let sections = raw_sections
        .into_iter()
        .zip(paths)
        .map(|(patch, path)| FileSection {
            path,
            document: parse_unified_patch(&patch).unwrap_or_default(),
        })
        .collect();
    (rows, sections)
}

fn patch_file_path(line: &str, marker: &str, prefix: &str) -> Option<PathBuf> {
    let candidate = line.strip_prefix(marker)?;
    let candidate = candidate.strip_prefix(prefix).unwrap_or(candidate);
    (candidate != "/dev/null").then(|| PathBuf::from(candidate))
}

fn hunk_starts(header: &str) -> Option<(u32, u32)> {
    let mut fields = header.split_whitespace();
    (fields.next()? == "@@").then_some(())?;
    Some((
        range_start(fields.next()?, '-')?,
        range_start(fields.next()?, '+')?,
    ))
}

fn range_start(range: &str, prefix: char) -> Option<u32> {
    range.strip_prefix(prefix)?.split(',').next()?.parse().ok()
}

fn include_line(range: &mut Option<diffo_highlight::LineRange>, line: Option<u32>) {
    let Some(line) = line else {
        return;
    };
    match range {
        Some(range) => {
            range.start = range.start.min(line);
            range.end = range.end.max(line);
        }
        None => *range = Some(diffo_highlight::LineRange::new(line, line)),
    }
}

fn file_line_count(document: &DiffDocument) -> usize {
    document
        .hunks
        .iter()
        .flat_map(|hunk| &hunk.blocks)
        .flat_map(|block| match block {
            DiffBlock::Context(lines) => lines.iter().collect::<Vec<_>>(),
            DiffBlock::Change { removed, added, .. } => {
                removed.iter().chain(added).collect::<Vec<_>>()
            }
            DiffBlock::Meta(_) => Vec::new(),
        })
        .flat_map(|line| [line.old_number, line.new_number])
        .flatten()
        .max()
        .map_or(0, |line| line as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepares_multiple_files_as_one_hunk_only_document() {
        let patch = Arc::<str>::from(concat!(
            "diff --git a/src/a.rs b/src/a.rs\n",
            "--- a/src/a.rs\n",
            "+++ b/src/a.rs\n",
            "@@ -1 +1 @@\n",
            "-fn old() {}\n",
            "+fn new() {}\n",
            "diff --git a/README.md b/README.md\n",
            "--- a/README.md\n",
            "+++ b/README.md\n",
            "@@ -1 +1 @@\n",
            "-old\n",
            "+new\n",
        ));
        let outcome = prepare(
            PrepareRequest {
                id: 7,
                commit_id: "1234567890".to_owned(),
                summary: "change both".to_owned(),
                patch,
                target_scroll: 0,
                viewport_rows: 40,
                window_viewports: 3,
            },
            &SyntaxHighlighter::new(),
        );

        assert_eq!(outcome.id, 7);
        assert_eq!(outcome.prepared.lines.len(), 12);
        assert_eq!(
            outcome.prepared.title().to_string(),
            " 1234567 · change both "
        );
        assert!(outcome.prepared.syntax_ready(0, 40));
    }

    #[test]
    fn parses_root_hunk_ranges() {
        assert_eq!(hunk_starts("@@ -0,0 +1,2 @@"), Some((0, 1)));
    }
}
