use diffo_diff::RenderLine;

use super::state::{AnchorRow, HighlightCache, ScrollAnchor};
use crate::diff::DiffViewMode;

impl ScrollAnchor {
    pub(in crate::diff) fn capture(
        cache: &HighlightCache,
        mode: DiffViewMode,
        first_row: usize,
    ) -> Self {
        let row_count = projection_len(cache, mode);
        Self {
            rows: (first_row..row_count)
                .take(16)
                .filter_map(|index| {
                    anchor_row(cache, mode, index).map(|row| (index - first_row, index, row))
                })
                .collect(),
        }
    }

    pub(in crate::diff) fn resolve(
        &self,
        cache: &HighlightCache,
        mode: DiffViewMode,
    ) -> Option<usize> {
        let row_count = projection_len(cache, mode);
        self.rows
            .iter()
            .find_map(|(viewport_offset, old_index, anchor)| {
                (0..row_count)
                    .filter(|index| anchor.matches(cache, mode, *index))
                    .min_by_key(|index| index.abs_diff(*old_index))
                    .map(|index| index.saturating_sub(*viewport_offset))
            })
    }
}

impl AnchorRow {
    fn matches(&self, cache: &HighlightCache, mode: DiffViewMode, index: usize) -> bool {
        match (self, mode) {
            (Self::Inline { kind, text }, DiffViewMode::Inline) => cache
                .inline
                .get(index)
                .is_some_and(|row| row.kind == *kind && row.text == *text),
            (Self::SideBySide { old, new }, DiffViewMode::SideBySide) => {
                cache.side_by_side.get(index).is_some_and(|row| {
                    side_line_matches(old.as_ref(), row.old.as_ref())
                        && side_line_matches(new.as_ref(), row.new.as_ref())
                })
            }
            (Self::Hunk { kind, text }, DiffViewMode::Hunk) => cache
                .hunk
                .get(index)
                .is_some_and(|row| row.kind == *kind && row.text == *text),
            _ => false,
        }
    }
}

fn side_line_matches(
    expected: Option<&(diffo_diff::RowKind, String)>,
    actual: Option<&RenderLine>,
) -> bool {
    match (expected, actual) {
        (Some((kind, text)), Some(actual)) => actual.kind == *kind && actual.text == *text,
        (None, None) => true,
        _ => false,
    }
}

fn projection_len(cache: &HighlightCache, mode: DiffViewMode) -> usize {
    match mode {
        DiffViewMode::Inline => cache.inline.len(),
        DiffViewMode::SideBySide => cache.side_by_side.len(),
        DiffViewMode::Hunk => cache.hunk.len(),
    }
}

pub(super) fn first_change(cache: &HighlightCache, mode: DiffViewMode) -> Option<usize> {
    match mode {
        DiffViewMode::Inline => cache.inline_changes.first().map(|change| change.first),
        DiffViewMode::SideBySide => cache
            .side_by_side_changes
            .first()
            .map(|change| change.first),
        DiffViewMode::Hunk => cache.hunk_changes.first().map(|change| change.first),
    }
}

fn anchor_row(cache: &HighlightCache, mode: DiffViewMode, index: usize) -> Option<AnchorRow> {
    match mode {
        DiffViewMode::Inline => cache.inline.get(index).map(|row| AnchorRow::Inline {
            kind: row.kind,
            text: row.text.clone(),
        }),
        DiffViewMode::SideBySide => {
            cache
                .side_by_side
                .get(index)
                .map(|row| AnchorRow::SideBySide {
                    old: row.old.as_ref().map(|line| (line.kind, line.text.clone())),
                    new: row.new.as_ref().map(|line| (line.kind, line.text.clone())),
                })
        }
        DiffViewMode::Hunk => cache.hunk.get(index).map(|row| AnchorRow::Hunk {
            kind: row.kind,
            text: row.text.clone(),
        }),
    }
}
