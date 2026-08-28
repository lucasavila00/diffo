use std::collections::BTreeSet;

use diffo_diff::{ChangeRegion, DiffDocument};
use diffo_highlight::{
    HIGHLIGHT_LOOKBEHIND_LINES, HighlightWindowRequest, HighlightedDiff, LineRange,
    MAX_HIGHLIGHT_BYTES_PER_SIDE, SyntaxHighlighter,
};
use diffo_ui::text_view::{SyntaxCoverage, centered_window};

use super::{ProjectionHighlightRequest, diff_file_lines};
use crate::diff::{HunkRow, MAX_HIGHLIGHT_FILE_LINES, ReviewHunkSegment};

pub(super) struct AggregateSyntax {
    pub(super) styles: Vec<HighlightedDiff>,
    pub(super) old_coverage: Vec<SyntaxCoverage>,
    pub(super) new_coverage: Vec<SyntaxCoverage>,
    pub(super) row_coverage: SyntaxCoverage,
    pub(super) enabled: bool,
    #[cfg(test)]
    pub(super) lines_processed: usize,
}

pub(super) fn highlight_aggregate(
    highlighter: &SyntaxHighlighter,
    segments: &[ReviewHunkSegment],
    documents: &[DiffDocument],
    rows: &[HunkRow],
    changes: &[ChangeRegion],
    request: ProjectionHighlightRequest,
) -> AggregateSyntax {
    let enabled = segments.iter().zip(documents).any(|(segment, document)| {
        segment.selection.file_path().is_some()
            && diff_file_lines(document) < MAX_HIGHLIGHT_FILE_LINES
    });
    let target = request
        .target_scroll
        .or_else(|| changes.first().map(|change| change.first))
        .unwrap_or(0);
    let window = centered_window(
        target,
        rows.len(),
        request.viewport_rows,
        request.prefetch_viewports.max(1),
    );
    let visible = rows
        .iter()
        .skip(window.start)
        .take(window.len())
        .filter_map(|row| row.segment)
        .collect::<BTreeSet<_>>();
    let byte_budget = MAX_HIGHLIGHT_BYTES_PER_SIDE / visible.len().max(1);
    let mut styles = vec![HighlightedDiff::default(); segments.len()];
    let mut old_coverage = vec![SyntaxCoverage::default(); segments.len()];
    let mut new_coverage = vec![SyntaxCoverage::default(); segments.len()];
    #[cfg(test)]
    let mut lines_processed = 0_usize;

    for segment_index in visible {
        let Some((segment, document)) = segments
            .get(segment_index)
            .zip(documents.get(segment_index))
        else {
            continue;
        };
        let Some(path) = segment.selection.file_path() else {
            continue;
        };
        if diff_file_lines(document) >= MAX_HIGHLIGHT_FILE_LINES {
            continue;
        }
        let (old, new) = segment_ranges(rows, &window, segment_index);
        let window = highlighter.highlight_window(
            path,
            document,
            HighlightWindowRequest {
                old,
                new,
                lookbehind_lines: HIGHLIGHT_LOOKBEHIND_LINES,
                maximum_bytes_per_side: byte_budget,
            },
        );
        old_coverage[segment_index] = SyntaxCoverage::from_range(window.old_coverage);
        new_coverage[segment_index] = SyntaxCoverage::from_range(window.new_coverage);
        #[cfg(test)]
        {
            lines_processed = lines_processed
                .saturating_add(window.old_lines_processed)
                .saturating_add(window.new_lines_processed);
        }
        styles[segment_index] = window.styles;
    }

    AggregateSyntax {
        styles,
        old_coverage,
        new_coverage,
        row_coverage: SyntaxCoverage::from_range(row_range(&window)),
        enabled,
        #[cfg(test)]
        lines_processed,
    }
}

fn segment_ranges(
    rows: &[HunkRow],
    window: &std::ops::Range<usize>,
    segment: usize,
) -> (Option<LineRange>, Option<LineRange>) {
    let mut old = None;
    let mut new = None;
    for row in rows
        .iter()
        .skip(window.start)
        .take(window.len())
        .filter(|row| row.segment == Some(segment))
    {
        include_line(&mut old, row.old_number);
        include_line(&mut new, row.new_number);
    }
    (old, new)
}

fn include_line(range: &mut Option<LineRange>, line: Option<u32>) {
    let Some(line) = line else {
        return;
    };
    match range {
        Some(range) => {
            range.start = range.start.min(line);
            range.end = range.end.max(line);
        }
        None => *range = Some(LineRange::new(line, line)),
    }
}

fn row_range(window: &std::ops::Range<usize>) -> Option<LineRange> {
    (!window.is_empty()).then(|| {
        LineRange::new(
            u32::try_from(window.start).unwrap_or(u32::MAX),
            u32::try_from(window.end.saturating_sub(1)).unwrap_or(u32::MAX),
        )
    })
}
