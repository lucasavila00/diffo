use diffo_diff::{ChangeRegion, DiffBlock, DiffDocument, RenderLine, RowKind, SideBySideRow};
use diffo_highlight::{
    HIGHLIGHT_LOOKBEHIND_LINES, HighlightWindowRequest, HighlightedWindow, LineRange,
    MAX_HIGHLIGHT_BYTES_PER_SIDE, MAX_HIGHLIGHT_FILE_LINES, SyntaxHighlighter,
};
use diffo_ui::text_view::centered_window;

use crate::diff::{DiffKey, DiffViewMode, HunkRow};

#[derive(Clone, Copy)]
pub(super) struct ProjectionHighlightRequest {
    pub(super) viewport_rows: usize,
    pub(super) mode: DiffViewMode,
    pub(super) target_scroll: Option<usize>,
    pub(super) prefetch_viewports: usize,
}

pub(super) fn highlight_visible_window(
    highlighter: &SyntaxHighlighter,
    key: &DiffKey,
    document: &DiffDocument,
    eligible: bool,
    old: Option<LineRange>,
    new: Option<LineRange>,
) -> Option<HighlightedWindow> {
    key.selection.file_path().and_then(|path| {
        eligible.then(|| {
            highlighter.highlight_window(
                path,
                document,
                HighlightWindowRequest {
                    old,
                    new,
                    lookbehind_lines: HIGHLIGHT_LOOKBEHIND_LINES,
                    maximum_bytes_per_side: MAX_HIGHLIGHT_BYTES_PER_SIDE,
                },
            )
        })
    })
}

pub(super) fn projection_highlight_ranges(
    inline: &[RenderLine],
    inline_changes: &[ChangeRegion],
    side_by_side: &[SideBySideRow],
    side_by_side_changes: &[ChangeRegion],
    hunk: &[HunkRow],
    hunk_changes: &[ChangeRegion],
    request: ProjectionHighlightRequest,
) -> (Option<LineRange>, Option<LineRange>) {
    let window_viewports = request.prefetch_viewports.max(1);
    debug_assert_eq!(window_viewports % 2, 1);
    let inline_target = request
        .target_scroll
        .filter(|_| request.mode == DiffViewMode::Inline)
        .or_else(|| inline_changes.first().map(|change| change.first))
        .unwrap_or(0);
    let side_target = request
        .target_scroll
        .filter(|_| request.mode == DiffViewMode::SideBySide)
        .or_else(|| side_by_side_changes.first().map(|change| change.first))
        .unwrap_or(0);
    let hunk_target = request
        .target_scroll
        .filter(|_| request.mode == DiffViewMode::Hunk)
        .or_else(|| hunk_changes.first().map(|change| change.first))
        .unwrap_or(0);
    let inline_window = centered_window(
        inline_target,
        inline.len(),
        request.viewport_rows,
        window_viewports,
    );
    let side_window = centered_window(
        side_target,
        side_by_side.len(),
        request.viewport_rows,
        window_viewports,
    );
    let hunk_window = centered_window(
        hunk_target,
        hunk.len(),
        request.viewport_rows,
        window_viewports,
    );
    let mut old = None;
    let mut new = None;
    let include_inline = request.target_scroll.is_none() || request.mode == DiffViewMode::Inline;
    for row in inline
        .iter()
        .skip(inline_window.start)
        .take(inline_window.len())
        .filter(|_| include_inline)
    {
        match row.kind {
            RowKind::Removed => include_line(&mut old, row.number),
            RowKind::Added | RowKind::Context | RowKind::Changed | RowKind::Conflict => {
                include_line(&mut new, row.number);
            }
            RowKind::Header | RowKind::Meta => {}
        }
    }
    let include_side = request.target_scroll.is_none() || request.mode == DiffViewMode::SideBySide;
    for row in side_by_side
        .iter()
        .skip(side_window.start)
        .take(side_window.len())
        .filter(|_| include_side)
    {
        include_line(&mut old, row.old.as_ref().and_then(|line| line.number));
        include_line(&mut new, row.new.as_ref().and_then(|line| line.number));
    }
    let include_hunk = request.target_scroll.is_none() || request.mode == DiffViewMode::Hunk;
    for row in hunk
        .iter()
        .skip(hunk_window.start)
        .take(hunk_window.len())
        .filter(|_| include_hunk)
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

pub(in crate::diff) fn should_syntax_highlight(document: &DiffDocument) -> bool {
    diff_file_lines(document) < MAX_HIGHLIGHT_FILE_LINES
}

pub(in crate::diff) fn diff_file_lines(document: &DiffDocument) -> usize {
    let mut maximum = 0;
    let mut include = |line: &diffo_diff::DiffLine| {
        maximum = maximum.max(
            line.old_number
                .into_iter()
                .chain(line.new_number)
                .max()
                .map_or(0, |number| number as usize),
        );
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
    maximum
}
