use diffo_diff::{ChangeRegion, DiffBlock, DiffDocument, RowKind};

use super::state::{HunkRow, ReviewHunkSegment, ReviewSelection};

const COMPACT_CONTEXT_LINES: usize = 3;

pub(super) struct AggregateHunk {
    pub(super) document: DiffDocument,
    pub(super) rows: Vec<HunkRow>,
    pub(super) targets: Vec<(ReviewSelection, usize)>,
}

pub(super) fn aggregate_hunk_rows(segments: &[ReviewHunkSegment]) -> Option<AggregateHunk> {
    let mut rows = Vec::new();
    let mut targets = Vec::new();
    for segment in segments {
        let document = crate::diff::parse_unified_patch(&segment.patch).ok()?;
        let segment_start = rows.len();
        let metadata = segment
            .patch
            .lines()
            .take_while(|line| !line.starts_with("@@ "))
            .map(|line| HunkRow {
                prefix: None,
                text: line.to_owned(),
                kind: RowKind::Meta,
                old_number: None,
                new_number: None,
            })
            .collect::<Vec<_>>();
        rows.extend(metadata);
        let target = rows.len();
        for hunk in &document.hunks {
            rows.extend(compact_hunk(hunk, segment.mark_conflicts));
        }
        if document.hunks.is_empty() {
            targets.push((segment.selection.clone(), segment_start));
        } else {
            targets.push((segment.selection.clone(), target));
        }
    }
    Some(AggregateHunk {
        document: DiffDocument::default(),
        rows,
        targets,
    })
}

fn compact_hunk(hunk: &diffo_diff::Hunk, mark_conflicts: bool) -> Vec<HunkRow> {
    let mut body = Vec::new();
    for block in &hunk.blocks {
        match block {
            DiffBlock::Context(lines) => body.extend(lines.iter().map(|line| HunkRow {
                prefix: Some(' '),
                text: line.text.clone(),
                kind: hunk_row_kind(&line.text, RowKind::Context, mark_conflicts),
                old_number: line.old_number,
                new_number: line.new_number,
            })),
            DiffBlock::Change { removed, added, .. } => {
                body.extend(removed.iter().map(|line| HunkRow {
                    prefix: Some('-'),
                    text: line.text.clone(),
                    kind: hunk_row_kind(&line.text, RowKind::Removed, mark_conflicts),
                    old_number: line.old_number,
                    new_number: None,
                }));
                body.extend(added.iter().map(|line| HunkRow {
                    prefix: Some('+'),
                    text: line.text.clone(),
                    kind: hunk_row_kind(&line.text, RowKind::Added, mark_conflicts),
                    old_number: None,
                    new_number: line.new_number,
                }));
            }
            DiffBlock::Meta(text) => body.push(HunkRow {
                prefix: None,
                text: text.clone(),
                kind: RowKind::Meta,
                old_number: None,
                new_number: None,
            }),
        }
    }
    let changes = body
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            matches!(
                row.kind,
                RowKind::Added | RowKind::Removed | RowKind::Conflict
            )
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let mut windows = Vec::<(usize, usize)>::new();
    for change in changes {
        let start = change.saturating_sub(COMPACT_CONTEXT_LINES);
        let end = change
            .saturating_add(COMPACT_CONTEXT_LINES)
            .saturating_add(1)
            .min(body.len());
        if let Some((_, previous_end)) = windows.last_mut()
            && start <= *previous_end
        {
            *previous_end = (*previous_end).max(end);
        } else {
            windows.push((start, end));
        }
    }
    let mut rows = Vec::new();
    for (start, end) in windows {
        let visible = &body[start..end];
        let old_start = visible
            .iter()
            .find_map(|row| row.old_number)
            .unwrap_or(hunk.old_start);
        let new_start = visible
            .iter()
            .find_map(|row| row.new_number)
            .unwrap_or(hunk.new_start);
        let old_count = visible
            .iter()
            .filter(|row| row.old_number.is_some())
            .count();
        let new_count = visible
            .iter()
            .filter(|row| row.new_number.is_some())
            .count();
        rows.push(HunkRow {
            prefix: None,
            text: format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@"),
            kind: RowKind::Header,
            old_number: None,
            new_number: None,
        });
        rows.extend_from_slice(visible);
    }
    rows
}

pub(super) fn hunk_rows(document: &DiffDocument, mark_conflicts: bool) -> Vec<HunkRow> {
    let mut rows = Vec::new();
    for hunk in &document.hunks {
        rows.push(HunkRow {
            prefix: None,
            text: hunk.header.clone(),
            kind: RowKind::Header,
            old_number: None,
            new_number: None,
        });
        for block in &hunk.blocks {
            match block {
                DiffBlock::Context(lines) => rows.extend(lines.iter().map(|line| HunkRow {
                    prefix: Some(' '),
                    text: line.text.clone(),
                    kind: hunk_row_kind(&line.text, RowKind::Context, mark_conflicts),
                    old_number: line.old_number,
                    new_number: line.new_number,
                })),
                DiffBlock::Change { removed, added, .. } => {
                    rows.extend(removed.iter().map(|line| HunkRow {
                        prefix: Some('-'),
                        text: line.text.clone(),
                        kind: hunk_row_kind(&line.text, RowKind::Removed, mark_conflicts),
                        old_number: line.old_number,
                        new_number: line.new_number,
                    }));
                    rows.extend(added.iter().map(|line| HunkRow {
                        prefix: Some('+'),
                        text: line.text.clone(),
                        kind: hunk_row_kind(&line.text, RowKind::Added, mark_conflicts),
                        old_number: line.old_number,
                        new_number: line.new_number,
                    }));
                }
                DiffBlock::Meta(text) => rows.push(HunkRow {
                    prefix: None,
                    text: text.clone(),
                    kind: RowKind::Meta,
                    old_number: None,
                    new_number: None,
                }),
            }
        }
    }
    rows
}

/// Preserves the compact patch that represents a complete immutable change.
///
/// File selections use the parsed document above, whose hunk mode can include
/// the full file content supplied by the mutable Diff source. A complete
/// change instead retains its Git patch headers and only its recorded context.
pub(super) fn complete_change_rows(raw_patch: &str, mark_conflicts: bool) -> Vec<HunkRow> {
    let mut rows = Vec::new();
    let mut old_line = 0_u32;
    let mut new_line = 0_u32;
    let mut in_hunk = false;

    for line in raw_patch.lines() {
        if let Some((old, new)) = hunk_starts(line) {
            old_line = old;
            new_line = new;
            in_hunk = true;
            rows.push(HunkRow {
                prefix: None,
                text: line.to_owned(),
                kind: RowKind::Header,
                old_number: None,
                new_number: None,
            });
            continue;
        }
        if in_hunk {
            if let Some(text) = line.strip_prefix(' ') {
                rows.push(HunkRow {
                    prefix: Some(' '),
                    text: text.to_owned(),
                    kind: hunk_row_kind(text, RowKind::Context, mark_conflicts),
                    old_number: Some(old_line),
                    new_number: Some(new_line),
                });
                old_line = old_line.saturating_add(1);
                new_line = new_line.saturating_add(1);
                continue;
            }
            if let Some(text) = line.strip_prefix('-') {
                rows.push(HunkRow {
                    prefix: Some('-'),
                    text: text.to_owned(),
                    kind: hunk_row_kind(text, RowKind::Removed, mark_conflicts),
                    old_number: Some(old_line),
                    new_number: None,
                });
                old_line = old_line.saturating_add(1);
                continue;
            }
            if let Some(text) = line.strip_prefix('+') {
                rows.push(HunkRow {
                    prefix: Some('+'),
                    text: text.to_owned(),
                    kind: hunk_row_kind(text, RowKind::Added, mark_conflicts),
                    old_number: None,
                    new_number: Some(new_line),
                });
                new_line = new_line.saturating_add(1);
                continue;
            }
            in_hunk = false;
        }
        rows.push(HunkRow {
            prefix: None,
            text: line.to_owned(),
            kind: RowKind::Meta,
            old_number: None,
            new_number: None,
        });
    }
    rows
}

pub(super) fn hunk_change_regions(rows: &[HunkRow]) -> Vec<ChangeRegion> {
    let mut changes = Vec::<ChangeRegion>::new();
    for (index, row) in rows.iter().enumerate() {
        if !matches!(
            row.kind,
            RowKind::Added | RowKind::Removed | RowKind::Conflict
        ) {
            continue;
        }
        if let Some(change) = changes.last_mut()
            && change.last.saturating_add(1) == index
        {
            change.last = index;
        } else {
            changes.push(ChangeRegion {
                first: index,
                last: index,
            });
        }
    }
    changes
}

fn hunk_row_kind(text: &str, fallback: RowKind, mark_conflicts: bool) -> RowKind {
    if mark_conflicts
        && (text.starts_with("<<<<<<<")
            || text.starts_with("|||||||")
            || text.starts_with("=======")
            || text.starts_with(">>>>>>>"))
    {
        RowKind::Conflict
    } else {
        fallback
    }
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
