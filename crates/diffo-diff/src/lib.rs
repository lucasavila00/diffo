use anyhow::{Context, Result, bail};
use similar::{Algorithm, DiffOp, capture_diff_slices};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiffDocument {
    pub hunks: Vec<Hunk>,
    pub binary: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hunk {
    pub header: String,
    pub old_start: u32,
    pub new_start: u32,
    pub blocks: Vec<DiffBlock>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiffBlock {
    Context(Vec<DiffLine>),
    Change {
        removed: Vec<DiffLine>,
        added: Vec<DiffLine>,
        alignment: Vec<LinePair>,
    },
    Meta(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffLine {
    pub old_number: Option<u32>,
    pub new_number: Option<u32>,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinePair {
    pub old: Option<DiffLine>,
    pub new: Option<DiffLine>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowKind {
    Header,
    Context,
    Removed,
    Added,
    Changed,
    Meta,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderLine {
    pub number: Option<u32>,
    pub text: String,
    pub kind: RowKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SideBySideRow {
    pub old: Option<RenderLine>,
    pub new: Option<RenderLine>,
    pub kind: RowKind,
}

/// Parse a file-scoped unified Git patch.
///
/// # Errors
///
/// Returns an error when a hunk header is malformed.
pub fn parse_unified_patch(patch: &str) -> Result<DiffDocument> {
    if patch.contains("GIT binary patch")
        || patch.lines().any(|line| line.starts_with("Binary files "))
    {
        return Ok(DiffDocument {
            binary: true,
            ..DiffDocument::default()
        });
    }

    let mut document = DiffDocument::default();
    let mut current = None;
    for line in patch.lines() {
        if line.starts_with("@@@") || line.starts_with("diff --cc ") {
            bail!("combined diffs are not supported yet");
        }
        if line.starts_with("@@ ") {
            if let Some(hunk) = current.take() {
                document.hunks.push(finish_hunk(hunk));
            }
            let (old_start, new_start) = parse_hunk_header(line)?;
            current = Some(HunkBuilder::new(line, old_start, new_start));
        } else if let Some(hunk) = current.as_mut() {
            hunk.push(line);
        }
    }
    if let Some(hunk) = current {
        document.hunks.push(finish_hunk(hunk));
    }
    Ok(document)
}

#[must_use]
pub fn inline_rows(document: &DiffDocument) -> Vec<RenderLine> {
    let mut rows = Vec::new();
    for hunk in &document.hunks {
        rows.push(RenderLine {
            number: None,
            text: hunk.header.clone(),
            kind: RowKind::Header,
        });
        for block in &hunk.blocks {
            match block {
                DiffBlock::Context(lines) => rows.extend(lines.iter().map(|line| RenderLine {
                    number: line.new_number,
                    text: line.text.clone(),
                    kind: RowKind::Context,
                })),
                DiffBlock::Change { removed, added, .. } => {
                    rows.extend(removed.iter().map(|line| RenderLine {
                        number: line.old_number,
                        text: line.text.clone(),
                        kind: RowKind::Removed,
                    }));
                    rows.extend(added.iter().map(|line| RenderLine {
                        number: line.new_number,
                        text: line.text.clone(),
                        kind: RowKind::Added,
                    }));
                }
                DiffBlock::Meta(text) => rows.push(RenderLine {
                    number: None,
                    text: text.clone(),
                    kind: RowKind::Meta,
                }),
            }
        }
    }
    rows
}

#[must_use]
pub fn side_by_side_rows(document: &DiffDocument) -> Vec<SideBySideRow> {
    let mut rows = Vec::new();
    for hunk in &document.hunks {
        let header = RenderLine {
            number: None,
            text: hunk.header.clone(),
            kind: RowKind::Header,
        };
        rows.push(SideBySideRow {
            old: Some(header.clone()),
            new: Some(header),
            kind: RowKind::Header,
        });
        for block in &hunk.blocks {
            match block {
                DiffBlock::Context(lines) => rows.extend(lines.iter().map(|line| SideBySideRow {
                    old: Some(RenderLine {
                        number: line.old_number,
                        text: line.text.clone(),
                        kind: RowKind::Context,
                    }),
                    new: Some(RenderLine {
                        number: line.new_number,
                        text: line.text.clone(),
                        kind: RowKind::Context,
                    }),
                    kind: RowKind::Context,
                })),
                DiffBlock::Change { alignment, .. } => {
                    rows.extend(alignment.iter().map(|pair| SideBySideRow {
                        old: pair.old.as_ref().map(|line| RenderLine {
                            number: line.old_number,
                            text: line.text.clone(),
                            kind: RowKind::Removed,
                        }),
                        new: pair.new.as_ref().map(|line| RenderLine {
                            number: line.new_number,
                            text: line.text.clone(),
                            kind: RowKind::Added,
                        }),
                        kind: RowKind::Changed,
                    }));
                }
                DiffBlock::Meta(text) => rows.push(SideBySideRow {
                    old: Some(RenderLine {
                        number: None,
                        text: text.clone(),
                        kind: RowKind::Meta,
                    }),
                    new: None,
                    kind: RowKind::Meta,
                }),
            }
        }
    }
    rows
}

struct HunkBuilder {
    header: String,
    old_start: u32,
    new_start: u32,
    old_line: u32,
    new_line: u32,
    blocks: Vec<DiffBlock>,
    context: Vec<DiffLine>,
    removed: Vec<DiffLine>,
    added: Vec<DiffLine>,
}

impl HunkBuilder {
    fn new(header: &str, old_start: u32, new_start: u32) -> Self {
        Self {
            header: header.to_owned(),
            old_start,
            new_start,
            old_line: old_start,
            new_line: new_start,
            blocks: Vec::new(),
            context: Vec::new(),
            removed: Vec::new(),
            added: Vec::new(),
        }
    }

    fn push(&mut self, line: &str) {
        if let Some(text) = line.strip_prefix(' ') {
            self.flush_change();
            self.context.push(DiffLine {
                old_number: Some(self.old_line),
                new_number: Some(self.new_line),
                text: text.to_owned(),
            });
            self.old_line += 1;
            self.new_line += 1;
        } else if let Some(text) = line.strip_prefix('-') {
            self.flush_context();
            self.removed.push(DiffLine {
                old_number: Some(self.old_line),
                new_number: None,
                text: text.to_owned(),
            });
            self.old_line += 1;
        } else if let Some(text) = line.strip_prefix('+') {
            self.flush_context();
            self.added.push(DiffLine {
                old_number: None,
                new_number: Some(self.new_line),
                text: text.to_owned(),
            });
            self.new_line += 1;
        } else if line.starts_with('\\') {
            self.flush_context();
            self.flush_change();
            self.blocks.push(DiffBlock::Meta(line.to_owned()));
        }
    }

    fn flush_context(&mut self) {
        if !self.context.is_empty() {
            self.blocks
                .push(DiffBlock::Context(std::mem::take(&mut self.context)));
        }
    }

    fn flush_change(&mut self) {
        if self.removed.is_empty() && self.added.is_empty() {
            return;
        }
        let removed = std::mem::take(&mut self.removed);
        let added = std::mem::take(&mut self.added);
        let alignment = align_lines(&removed, &added);
        self.blocks.push(DiffBlock::Change {
            removed,
            added,
            alignment,
        });
    }
}

fn finish_hunk(mut builder: HunkBuilder) -> Hunk {
    builder.flush_context();
    builder.flush_change();
    Hunk {
        header: builder.header,
        old_start: builder.old_start,
        new_start: builder.new_start,
        blocks: builder.blocks,
    }
}

fn parse_hunk_header(header: &str) -> Result<(u32, u32)> {
    let mut fields = header.split_whitespace();
    let marker = fields.next().context("hunk marker is missing")?;
    if marker != "@@" {
        bail!("invalid hunk marker");
    }
    let old = parse_range(fields.next().context("old hunk range is missing")?, '-')?;
    let new = parse_range(fields.next().context("new hunk range is missing")?, '+')?;
    Ok((old, new))
}

fn parse_range(range: &str, prefix: char) -> Result<u32> {
    range
        .strip_prefix(prefix)
        .context("hunk range has an invalid prefix")?
        .split(',')
        .next()
        .context("hunk start is missing")?
        .parse()
        .context("hunk start is not a number")
}

fn align_lines(removed: &[DiffLine], added: &[DiffLine]) -> Vec<LinePair> {
    let old = removed
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>();
    let new = added
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>();
    let mut pairs = Vec::new();
    for operation in capture_diff_slices(Algorithm::Myers, &old, &new) {
        match operation {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => extend_pairs(&mut pairs, removed, added, old_index, len, new_index, len),
            DiffOp::Delete {
                old_index, old_len, ..
            } => extend_pairs(&mut pairs, removed, added, old_index, old_len, 0, 0),
            DiffOp::Insert {
                new_index, new_len, ..
            } => extend_pairs(&mut pairs, removed, added, 0, 0, new_index, new_len),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => extend_pairs(
                &mut pairs, removed, added, old_index, old_len, new_index, new_len,
            ),
        }
    }
    pairs
}

fn extend_pairs(
    pairs: &mut Vec<LinePair>,
    removed: &[DiffLine],
    added: &[DiffLine],
    old_index: usize,
    old_len: usize,
    new_index: usize,
    new_len: usize,
) {
    for offset in 0..old_len.max(new_len) {
        pairs.push(LinePair {
            old: (offset < old_len).then(|| removed[old_index + offset].clone()),
            new: (offset < new_len).then(|| added[new_index + offset].clone()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{DiffBlock, RowKind, inline_rows, parse_unified_patch, side_by_side_rows};

    const PATCH: &str = "diff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1,4 +1,4 @@\n same\n-old one\n-old two\n+new one\n+new two\n end\n";

    #[test]
    fn parses_hunks_and_line_numbers() {
        let document = parse_unified_patch(PATCH).expect("patch should parse");

        assert_eq!(document.hunks.len(), 1);
        assert_eq!(document.hunks[0].old_start, 1);
        assert_eq!(document.hunks[0].new_start, 1);
        assert_eq!(document.hunks[0].blocks.len(), 3);
        let DiffBlock::Change {
            removed,
            added,
            alignment,
        } = &document.hunks[0].blocks[1]
        else {
            panic!("expected change block");
        };
        assert_eq!(removed[0].old_number, Some(2));
        assert_eq!(added[0].new_number, Some(2));
        assert_eq!(alignment.len(), 2);
    }

    #[test]
    fn projects_inline_and_side_by_side_rows() {
        let document = parse_unified_patch(PATCH).expect("patch should parse");
        let inline = inline_rows(&document);
        let side = side_by_side_rows(&document);

        assert_eq!(
            inline
                .iter()
                .filter(|row| row.kind == RowKind::Removed)
                .count(),
            2
        );
        assert_eq!(
            inline
                .iter()
                .filter(|row| row.kind == RowKind::Added)
                .count(),
            2
        );
        assert!(
            side.iter()
                .any(|row| row.old.is_some() && row.new.is_some())
        );
    }

    #[test]
    fn detects_binary_and_rejects_combined_diff() {
        assert!(
            parse_unified_patch("Binary files a/x and b/x differ")
                .expect("binary")
                .binary
        );
        assert!(parse_unified_patch("diff --cc file\n@@@ -1 -1 +1 @@@").is_err());
    }
}
