use diffo_diff::{ChangeRegion, DiffBlock, DiffDocument, RowKind};

use super::state::HunkRow;

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
