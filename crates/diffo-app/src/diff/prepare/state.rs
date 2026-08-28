use std::sync::{
    Arc,
    mpsc::{Receiver, Sender},
};

use crate::diff::FileKey;
use diffo_diff::{ChangeRegion, DiffDocument, RenderLine, RowKind, SideBySideRow};
use diffo_highlight::{HighlightedDiff, SyntaxHighlighter};
use diffo_ui::file_picker::FilePicker;
use diffo_ui::text_view::{PreparedVerticalScroll, SyntaxCoverage, TextSurfacePreparation};
use ratatui::{layout::Rect, text::Line};

pub struct Renderer {
    pub(in crate::diff) highlighter: Arc<SyntaxHighlighter>,
    pub(in crate::diff) highlighted: Option<HighlightCache>,
    pub(in crate::diff) prepared_cache: Vec<HighlightCache>,
    pub(in crate::diff) prepare_tx: Sender<PrepareRequest>,
    pub(in crate::diff) prepare_rx: Receiver<PrepareOutcome>,
    pub(in crate::diff) submitted: Vec<(DiffKey, Option<usize>)>,
    pub(in crate::diff) requested: Option<DiffKey>,
    pub(in crate::diff) requested_selection: Option<ReviewSelection>,
    pub(in crate::diff) displayed_selection: Option<ReviewSelection>,
    pub(in crate::diff) vertical_scroll: PreparedVerticalScroll,
    pub(in crate::diff) diff_viewport_rows: usize,
    pub(in crate::diff) failed: Option<DiffKey>,
    pub(in crate::diff) scrollbars: ScrollbarMetrics,
    pub(in crate::diff) scrollbar_drag: Option<ScrollbarAxis>,
    pub(in crate::diff) staged_picker: FilePicker<FileKey>,
    pub(in crate::diff) unstaged_picker: FilePicker<FileKey>,
    pub(in crate::diff) change_warnings: ChangeWarningAreas,
    pub(in crate::diff) content_revision: u64,
    #[cfg(test)]
    pub(in crate::diff) highlight_computations: usize,
}

pub(in crate::diff) struct HighlightCache {
    pub(in crate::diff) key: DiffKey,
    pub(in crate::diff) document: DiffDocument,
    pub(in crate::diff) inline: Vec<RenderLine>,
    pub(in crate::diff) side_by_side: Vec<SideBySideRow>,
    pub(in crate::diff) hunk: Vec<HunkRow>,
    pub(in crate::diff) inline_changes: Vec<ChangeRegion>,
    pub(in crate::diff) side_by_side_changes: Vec<ChangeRegion>,
    pub(in crate::diff) hunk_changes: Vec<ChangeRegion>,
    pub(in crate::diff) hunk_targets: Vec<(ReviewSelection, usize)>,
    pub(in crate::diff) highlighted: HighlightedDiff,
    pub(in crate::diff) syntax_highlighted: bool,
    pub(in crate::diff) highlighted_old_coverage: SyntaxCoverage,
    pub(in crate::diff) highlighted_new_coverage: SyntaxCoverage,
    #[cfg(test)]
    pub(in crate::diff) highlighted_lines_processed: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::diff) struct HunkRow {
    pub(in crate::diff) prefix: Option<char>,
    pub(in crate::diff) text: String,
    pub(in crate::diff) kind: RowKind,
    pub(in crate::diff) old_number: Option<u32>,
    pub(in crate::diff) new_number: Option<u32>,
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
    pub requested_explorer_file: Option<std::path::PathBuf>,
    pub displayed_explorer_file: Option<std::path::PathBuf>,
    pub requested_history_commit: Option<String>,
    pub selected_history_commit: Option<String>,
    pub displayed_history_commit: Option<String>,
    pub requested_history_file: Option<std::path::PathBuf>,
    pub selected_history_file: Option<std::path::PathBuf>,
    pub displayed_history_file: Option<std::path::PathBuf>,
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
    Hunk {
        kind: RowKind,
        text: String,
    },
}

pub(in crate::diff) struct ScrollAnchor {
    pub(in crate::diff) rows: Vec<(usize, usize, AnchorRow)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::diff) struct DiffKey {
    pub(in crate::diff) selection: ReviewSelection,
    pub(in crate::diff) title: Line<'static>,
    pub(in crate::diff) patch: Arc<str>,
    pub(in crate::diff) mark_conflicts: bool,
    pub(in crate::diff) mode: crate::diff::DiffViewMode,
    pub(in crate::diff) hunk_segments: Option<Arc<[ReviewHunkSegment]>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReviewHunkSegment {
    pub(crate) selection: ReviewSelection,
    pub(crate) patch: Arc<str>,
    pub(crate) mark_conflicts: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReviewHunkSet {
    pub(crate) id: String,
    pub(crate) title: Line<'static>,
    pub(crate) segments: Arc<[ReviewHunkSegment]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReviewDocument {
    pub(crate) selection: ReviewSelection,
    pub(crate) title: Line<'static>,
    pub(crate) patch: Arc<str>,
    pub(crate) mark_conflicts: bool,
    pub(crate) hunks: ReviewHunkSet,
}

pub(in crate::diff) struct ReviewPreparation {
    pub(in crate::diff) key: Option<DiffKey>,
    pub(in crate::diff) selection: Option<ReviewSelection>,
    pub(in crate::diff) area: Rect,
    pub(in crate::diff) undecorated: bool,
    pub(in crate::diff) mode: crate::diff::DiffViewMode,
    pub(in crate::diff) vertical: usize,
    pub(in crate::diff) horizontal: usize,
}

impl ReviewDocument {
    pub(in crate::diff) fn key(&self, file_view_mode: crate::diff::DiffViewMode) -> DiffKey {
        if file_view_mode == crate::diff::DiffViewMode::Hunk {
            DiffKey {
                mode: crate::diff::DiffViewMode::Hunk,
                selection: ReviewSelection::CompleteChange(self.hunks.id.clone()),
                title: self.hunks.title.clone(),
                patch: Arc::from(""),
                mark_conflicts: false,
                hunk_segments: Some(Arc::clone(&self.hunks.segments)),
            }
        } else {
            DiffKey {
                mode: file_view_mode,
                selection: self.selection.clone(),
                title: self.title.clone(),
                patch: Arc::clone(&self.patch),
                mark_conflicts: self.mark_conflicts,
                hunk_segments: None,
            }
        }
    }
}

/// The review content currently requested by a Diff renderer.
///
/// File selections identify the focused file. A complete change is the internal
/// identity of the aggregate hunk projection. The renderer keeps its displayed
/// selection until replacement content has prepared, so callers can change
/// either kind without exposing a partial review.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ReviewSelection {
    File(FileKey),
    HistoryFile {
        commit_id: String,
        path: std::path::PathBuf,
    },
    CompleteChange(String),
}

impl ReviewSelection {
    #[must_use]
    pub fn file_key(&self) -> Option<&FileKey> {
        match self {
            Self::File(file) => Some(file),
            Self::HistoryFile { .. } | Self::CompleteChange(_) => None,
        }
    }

    #[must_use]
    pub fn file_path(&self) -> Option<&std::path::Path> {
        match self {
            Self::File(file) => Some(&file.path),
            Self::HistoryFile { path, .. } => Some(path),
            Self::CompleteChange(_) => None,
        }
    }

    #[must_use]
    pub fn complete_change_id(&self) -> Option<&str> {
        match self {
            Self::File(_) | Self::HistoryFile { .. } => None,
            Self::CompleteChange(id) => Some(id),
        }
    }
}

impl DiffKey {
    pub(in crate::diff) fn workload_bytes(&self) -> usize {
        self.hunk_segments
            .as_ref()
            .map_or(self.patch.len(), |segments| {
                segments.iter().map(|segment| segment.patch.len()).sum()
            })
    }

    pub(in crate::diff) fn workload_lines(&self) -> usize {
        self.hunk_segments.as_ref().map_or_else(
            || self.patch.lines().count(),
            |segments| {
                segments
                    .iter()
                    .map(|segment| segment.patch.lines().count())
                    .sum()
            },
        )
    }
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
pub(in crate::diff) struct ChangeWarningAreas {
    pub(in crate::diff) previous: Option<Rect>,
    pub(in crate::diff) next: Option<Rect>,
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
    pub(in crate::diff) previous_change: Option<ChangeTarget>,
    pub(in crate::diff) next_change: Option<ChangeTarget>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::diff) struct ChangeTarget {
    pub(in crate::diff) scroll: usize,
    pub(in crate::diff) edge_row: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::diff) enum ScrollbarAxis {
    Vertical,
    Horizontal,
}

pub(in crate::diff) const MAX_SYNC_BYTES: usize = 64 * 1024;
pub(in crate::diff) const MAX_SYNC_LINES: usize = 500;
pub(in crate::diff) const PREPARED_BUFFER_CACHE_SIZE: usize = 4;
