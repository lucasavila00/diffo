use std::sync::{
    Arc,
    mpsc::{Receiver, Sender},
};

use crate::diff::FileKey;
use diffo_diff::{DiffDocument, RenderLine, RowKind, SideBySideRow};
use diffo_highlight::{HighlightedDiff, LineRange, SyntaxHighlighter};
use diffo_ui::file_picker::FilePicker;
use diffo_ui::text_view::TextSurfacePreparation;
use ratatui::{layout::Rect, text::Line};

pub struct Renderer {
    pub(in crate::diff) highlighter: Arc<SyntaxHighlighter>,
    pub(in crate::diff) highlighted: Option<HighlightCache>,
    pub(in crate::diff) prepared_cache: Vec<HighlightCache>,
    pub(in crate::diff) prepare_tx: Sender<PrepareRequest>,
    pub(in crate::diff) prepare_rx: Receiver<PrepareOutcome>,
    pub(in crate::diff) submitted: Vec<(DiffKey, Option<usize>)>,
    pub(in crate::diff) requested: Option<DiffKey>,
    pub(in crate::diff) requested_navigation_target: Option<usize>,
    pub(in crate::diff) diff_viewport_rows: usize,
    pub(in crate::diff) previous_diff_scroll: usize,
    pub(in crate::diff) failed: Option<DiffKey>,
    pub(in crate::diff) scrollbars: ScrollbarMetrics,
    pub(in crate::diff) scrollbar_drag: Option<ScrollbarAxis>,
    pub(in crate::diff) staged_picker: FilePicker<FileKey>,
    pub(in crate::diff) unstaged_picker: FilePicker<FileKey>,
    pub(in crate::diff) hunk_buttons: HunkButtonMetrics,
    pub(in crate::diff) content_revision: u64,
    #[cfg(test)]
    pub(in crate::diff) highlight_computations: usize,
}

pub(in crate::diff) struct HighlightCache {
    pub(in crate::diff) key: DiffKey,
    pub(in crate::diff) document: DiffDocument,
    pub(in crate::diff) inline: Vec<RenderLine>,
    pub(in crate::diff) side_by_side: Vec<SideBySideRow>,
    pub(in crate::diff) inline_changes: Vec<usize>,
    pub(in crate::diff) side_by_side_changes: Vec<usize>,
    pub(in crate::diff) highlighted: HighlightedDiff,
    pub(in crate::diff) syntax_highlighted: bool,
    pub(in crate::diff) highlighted_old_coverage: Vec<LineRange>,
    pub(in crate::diff) highlighted_new_coverage: Vec<LineRange>,
    #[cfg(test)]
    pub(in crate::diff) highlighted_lines_processed: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FramePreparation {
    pub maximum_vertical_scroll: usize,
    pub maximum_horizontal_scroll: usize,
    pub content_revision: u64,
    pub preparing: bool,
    pub syntax_ready: bool,
    pub viewport_transition: Option<ViewportTransition>,
    pub requested_file: Option<FileKey>,
    pub displayed_file: Option<FileKey>,
    pub text_surface: Option<TextSurfacePreparation>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ViewportTransition {
    pub vertical: usize,
    pub horizontal: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::diff) enum AnchorRow {
    Inline {
        kind: RowKind,
        text: String,
    },
    SideBySide {
        old: Option<(RowKind, String)>,
        new: Option<(RowKind, String)>,
    },
}

pub(in crate::diff) struct ScrollAnchor {
    pub(in crate::diff) rows: Vec<(usize, usize, AnchorRow)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::diff) struct DiffKey {
    pub(in crate::diff) file: FileKey,
    pub(in crate::diff) title: Line<'static>,
    pub(in crate::diff) patch: Arc<str>,
    pub(in crate::diff) mark_conflicts: bool,
    pub(in crate::diff) mode: crate::diff::DiffViewMode,
}

pub(in crate::diff) struct PrepareRequest {
    pub(in crate::diff) key: DiffKey,
    pub(in crate::diff) viewport_rows: usize,
    pub(in crate::diff) mode: crate::diff::DiffViewMode,
    pub(in crate::diff) target_scroll: Option<usize>,
    pub(in crate::diff) prefetch_viewports: usize,
}

pub(in crate::diff) struct PrepareOutcome {
    pub(in crate::diff) key: DiffKey,
    pub(in crate::diff) target_scroll: Option<usize>,
    pub(in crate::diff) cache: Option<HighlightCache>,
}

pub(in crate::diff) struct PrepareCommit {
    pub(in crate::diff) target_scroll: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::diff) struct ScrollbarMetrics {
    pub(in crate::diff) vertical_area: Rect,
    pub(in crate::diff) horizontal_area: Rect,
    pub(in crate::diff) rows: usize,
    pub(in crate::diff) columns: usize,
    pub(in crate::diff) viewport_columns: usize,
    pub(in crate::diff) maximum_vertical_scroll: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::diff) struct HunkButtonMetrics {
    pub(in crate::diff) previous: Option<(Rect, usize)>,
    pub(in crate::diff) next: Option<(Rect, usize)>,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::diff) struct DiffViewportMetrics {
    pub(in crate::diff) content_area: Rect,
    pub(in crate::diff) horizontal_area: Rect,
    pub(in crate::diff) viewport_rows: usize,
    pub(in crate::diff) viewport_columns: usize,
    pub(in crate::diff) rows: usize,
    pub(in crate::diff) columns: usize,
    pub(in crate::diff) maximum_vertical_scroll: usize,
    pub(in crate::diff) previous_change: Option<usize>,
    pub(in crate::diff) next_change: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::diff) enum ScrollbarAxis {
    Vertical,
    Horizontal,
}

pub(in crate::diff) const MAX_SYNC_BYTES: usize = 64 * 1024;
pub(in crate::diff) const MAX_SYNC_LINES: usize = 500;
pub(in crate::diff) const HIGHLIGHT_PREFETCH_VIEWPORTS: usize = 3;
pub(in crate::diff) const PREPARED_BUFFER_CACHE_SIZE: usize = 4;
