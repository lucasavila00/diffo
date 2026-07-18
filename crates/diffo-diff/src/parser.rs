use anyhow::{Context, Result, bail};
use similar::{Algorithm, DiffOp, capture_diff_slices};

use crate::{DiffBlock, DiffDocument, DiffLine, Hunk, LinePair};

/// Parse a file-scoped unified Git patch.
///
/// # Errors
///
/// Returns an error when a hunk header is malformed.
pub fn parse_unified_patch(patch: &str) -> Result<DiffDocument> {
    let mut document = DiffDocument::default();
    let mut current = None;
    for line in patch.lines() {
        if current.is_none() {
            if line == "GIT binary patch" || line.starts_with("Binary files ") {
                document.binary = true;
                return Ok(document);
            }
            if line.starts_with("@@@") || line.starts_with("diff --cc ") {
                bail!("combined diffs are not supported yet");
            }
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
