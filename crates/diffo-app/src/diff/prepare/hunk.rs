use diffo_diff::{ChangeRegion, DiffBlock, DiffDocument, RowKind};

use super::state::{HunkRow, ReviewHunkSegment, ReviewSelection};

const COMPACT_CONTEXT_LINES: usize = 3;

pub(super) struct AggregateHunk {
    pub(super) document: DiffDocument,
    pub(super) documents: Vec<DiffDocument>,
    pub(super) rows: Vec<HunkRow>,
    pub(super) targets: Vec<(ReviewSelection, std::ops::Range<usize>)>,
}

pub(super) fn aggregate_hunk_rows(segments: &[ReviewHunkSegment]) -> Option<AggregateHunk> {
    let mut rows = Vec::new();
    let mut targets = Vec::new();
    let mut documents = Vec::with_capacity(segments.len());
    for (segment_index, segment) in segments.iter().enumerate() {
        let document = crate::diff::parse_unified_patch(&segment.patch).ok()?;
        let segment_start = rows.len();
        let metadata = segment
            .patch
            .lines()
            .take_while(|line| !line.starts_with("@@ "))
            .map(|line| HunkRow {
                segment: Some(segment_index),
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
            rows.extend(compact_hunk(
                hunk,
                segment.mark_conflicts,
                Some(segment_index),
            ));
        }
        let target = if document.hunks.is_empty() {
            segment_start
        } else {
            target
        };
        targets.push((segment.selection.clone(), target..rows.len()));
        documents.push(document);
    }
    Some(AggregateHunk {
        document: DiffDocument::default(),
        documents,
        rows,
        targets,
    })
}

fn compact_hunk(
    hunk: &diffo_diff::Hunk,
    mark_conflicts: bool,
    segment: Option<usize>,
) -> Vec<HunkRow> {
    let mut body = Vec::new();
    for block in &hunk.blocks {
        match block {
            DiffBlock::Context(lines) => body.extend(lines.iter().map(|line| HunkRow {
                segment,
                prefix: Some(' '),
                text: line.text.clone(),
                kind: hunk_row_kind(&line.text, RowKind::Context, mark_conflicts),
                old_number: line.old_number,
                new_number: line.new_number,
            })),
            DiffBlock::Change { removed, added, .. } => {
                body.extend(removed.iter().map(|line| HunkRow {
                    segment,
                    prefix: Some('-'),
                    text: line.text.clone(),
                    kind: hunk_row_kind(&line.text, RowKind::Removed, mark_conflicts),
                    old_number: line.old_number,
                    new_number: None,
                }));
                body.extend(added.iter().map(|line| HunkRow {
                    segment,
                    prefix: Some('+'),
                    text: line.text.clone(),
                    kind: hunk_row_kind(&line.text, RowKind::Added, mark_conflicts),
                    old_number: None,
                    new_number: line.new_number,
                }));
            }
            DiffBlock::Meta(text) => body.push(HunkRow {
                segment,
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
            segment,
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
