use std::sync::{
    Arc,
    mpsc::{Receiver, SyncSender},
};

use diffo_app::{ChangeArea, FileKey, FileListScroll};
use diffo_diff::{DiffDocument, RenderLine, RowKind, SideBySideRow};
use diffo_highlight::{HighlightedDiff, LineRange, SyntaxHighlighter};
use ratatui::layout::Rect;

use crate::files::FileListMetrics;

pub struct Renderer {
    pub(super) highlighter: Arc<SyntaxHighlighter>,
    pub(super) highlighted: Option<HighlightCache>,
    pub(super) prepared_cache: Vec<HighlightCache>,
    pub(super) prepare_tx: SyncSender<PrepareRequest>,
    pub(super) prepare_rx: Receiver<PrepareOutcome>,
    pub(super) submitted: Vec<(DiffKey, Option<usize>)>,
    pub(super) requested: Option<DiffKey>,
    pub(super) pending_scroll: Option<usize>,
    pub(super) diff_viewport_rows: usize,
    pub(super) failed: Option<DiffKey>,
    pub(super) scrollbars: ScrollbarMetrics,
    pub(super) scrollbar_drag: Option<ScrollbarAxis>,
    pub(super) file_lists: FileListMetrics,
    pub(super) file_scrollbar_drag: Option<ChangeArea>,
    pub(super) hunk_buttons: HunkButtonMetrics,
    pub(super) content_revision: u64,
    pub(super) network_animation_tick: usize,
    #[cfg(test)]
    pub(super) highlight_computations: usize,
}

pub(super) struct HighlightCache {
    pub(super) key: DiffKey,
    pub(super) document: DiffDocument,
    pub(super) inline: Vec<RenderLine>,
    pub(super) side_by_side: Vec<SideBySideRow>,
    pub(super) inline_changes: Vec<usize>,
    pub(super) side_by_side_changes: Vec<usize>,
    pub(super) highlighted: HighlightedDiff,
    pub(super) syntax_highlighted: bool,
    pub(super) highlighted_old_coverage: Option<LineRange>,
    pub(super) highlighted_new_coverage: Option<LineRange>,
    #[cfg(test)]
    pub(super) highlighted_lines_processed: usize,
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
    pub file_list_scroll: FileListScroll,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ViewportTransition {
    pub vertical: usize,
    pub horizontal: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum AnchorRow {
    Inline {
        kind: RowKind,
        text: String,
    },
    SideBySide {
        old: Option<(RowKind, String)>,
        new: Option<(RowKind, String)>,
    },
}

pub(super) struct ScrollAnchor {
    pub(super) rows: Vec<(usize, usize, AnchorRow)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DiffKey {
    pub(super) file: FileKey,
    pub(super) patch: Arc<str>,
    pub(super) mark_conflicts: bool,
    pub(super) mode: diffo_app::DiffViewMode,
}

pub(super) struct PrepareRequest {
    pub(super) key: DiffKey,
    pub(super) viewport_rows: usize,
    pub(super) mode: diffo_app::DiffViewMode,
    pub(super) target_scroll: Option<usize>,
}

pub(super) struct PrepareOutcome {
    pub(super) key: DiffKey,
    pub(super) target_scroll: Option<usize>,
    pub(super) cache: Option<HighlightCache>,
}

pub(super) struct PrepareCommit {
    pub(super) target_scroll: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScrollbarMetrics {
    pub(super) vertical_area: Rect,
    pub(super) horizontal_area: Rect,
    pub(super) rows: usize,
    pub(super) columns: usize,
    pub(super) viewport_columns: usize,
    pub(super) maximum_vertical_scroll: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct HunkButtonMetrics {
    pub(super) previous: Option<(Rect, usize)>,
    pub(super) next: Option<(Rect, usize)>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DiffViewportMetrics {
    pub(super) content_area: Rect,
    pub(super) horizontal_area: Rect,
    pub(super) viewport_rows: usize,
    pub(super) viewport_columns: usize,
    pub(super) rows: usize,
    pub(super) columns: usize,
    pub(super) maximum_vertical_scroll: usize,
    pub(super) previous_change: Option<usize>,
    pub(super) next_change: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ScrollbarAxis {
    Vertical,
    Horizontal,
}

pub(super) const MAX_SYNC_BYTES: usize = 64 * 1024;
pub(super) const MAX_SYNC_LINES: usize = 500;
pub(super) const HIGHLIGHT_PREFETCH_VIEWPORTS: usize = 3;
pub(super) const PREPARED_BUFFER_CACHE_SIZE: usize = 4;
