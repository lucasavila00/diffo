use super::{
    DiffKey, DiffViewMode, FramePreparation, Renderer, ReviewFramePreparation, ReviewSelection,
    ViewportTransition,
};
use crate::diff::review_document::hunk_focus_target;
use diffo_ui::maximum_scroll;
use ratatui::layout::Rect;

impl Renderer {
    pub(super) fn review_scroll_bounds(
        &self,
        area: Rect,
        undecorated: bool,
        mode: DiffViewMode,
        vertical: usize,
    ) -> (usize, usize) {
        if undecorated {
            let viewport = self.full_screen_metrics(area, vertical);
            (viewport.maximum_vertical, viewport.maximum_horizontal)
        } else {
            let viewport = self.diff_viewport_metrics(mode, area, vertical);
            (
                viewport.maximum_vertical_scroll,
                maximum_scroll(viewport.columns, viewport.viewport_columns),
            )
        }
    }

    pub(super) fn focus_transition(
        &self,
        changed: bool,
        selection: Option<&ReviewSelection>,
    ) -> Option<ViewportTransition> {
        changed
            .then(|| {
                self.highlighted
                    .as_ref()
                    .and_then(|cache| selection.and_then(|focus| hunk_focus_target(cache, focus)))
            })
            .flatten()
            .map(|vertical| ViewportTransition {
                vertical,
                horizontal: 0,
            })
    }

    pub(super) fn frame_preparation(
        &self,
        requested: Option<&DiffKey>,
        prepared: ReviewFramePreparation,
    ) -> FramePreparation {
        FramePreparation {
            maximum_vertical_scroll: prepared.maximum_vertical,
            maximum_horizontal_scroll: prepared.maximum_horizontal,
            content_revision: self.content_revision,
            preparing: self.requested.as_ref() != self.displayed_key()
                || self.requested_selection != self.displayed_selection,
            syntax_ready: prepared.syntax_ready,
            viewport_transition: prepared.viewport_transition,
            requested_file: self
                .requested_selection
                .as_ref()
                .and_then(|selection| selection.file_key().cloned()),
            displayed_file: self
                .displayed_selection
                .as_ref()
                .and_then(|selection| selection.file_key().cloned()),
            requested_explorer_file: None,
            displayed_explorer_file: None,
            requested_history_commit: None,
            selected_history_commit: None,
            displayed_history_commit: None,
            requested_history_file: None,
            selected_history_file: None,
            displayed_history_file: None,
            text_surface: Some(self.text_surface_preparation(
                prepared.rendered_vertical,
                prepared.syntax_ready,
                prepared.target_scroll,
                requested,
            )),
        }
    }
}
