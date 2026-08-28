use super::{
    DiffKey, DiffViewMode, FramePreparation, Renderer, ReviewFramePreparation, ReviewSelection,
    ScrollAnchor, ViewportTransition,
};
use crate::diff::review_document::hunk_focus_target;
use diffo_ui::maximum_scroll;
use ratatui::layout::Rect;

impl Renderer {
    pub(super) fn review_anchor(
        &self,
        requested: Option<&DiffKey>,
        vertical: usize,
    ) -> Option<ScrollAnchor> {
        requested.and_then(|requested| {
            self.highlighted
                .as_ref()
                .filter(|cache| cache.key.selection == requested.selection)
                .map(|cache| ScrollAnchor::capture(cache, cache.key.mode, vertical))
        })
    }

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

    pub(super) fn focus_target(
        &self,
        changed: bool,
        requested: Option<&DiffKey>,
        selection: Option<&ReviewSelection>,
    ) -> Option<usize> {
        (changed && requested == self.displayed_key())
            .then(|| {
                self.highlighted
                    .as_ref()
                    .and_then(|cache| selection.and_then(|focus| hunk_focus_target(cache, focus)))
            })
            .flatten()
    }

    pub(super) fn commit_ready_focus(
        &self,
        requested: Option<&DiffKey>,
        mode: DiffViewMode,
        target: Option<usize>,
    ) -> Option<ViewportTransition> {
        let target = target?;
        (requested == self.displayed_key()
            && self.syntax_ready_for_viewport(self.displayed_mode(mode), target))
        .then_some(target)
        .map(|vertical| ViewportTransition {
            vertical,
            horizontal: 0,
        })
    }

    pub(super) fn review_preparation_target(
        &self,
        requested: Option<&DiffKey>,
        mode: DiffViewMode,
        vertical: usize,
        focus: Option<usize>,
    ) -> Option<usize> {
        focus
            .filter(|target| !self.syntax_ready_for_viewport(self.displayed_mode(mode), *target))
            .or_else(|| self.navigation_preparation_target(requested, mode))
            .or_else(|| self.syntax_target(requested, mode, vertical))
    }

    pub(super) fn prepared_viewport_transition(
        &mut self,
        requested: Option<&DiffKey>,
        mode: DiffViewMode,
        horizontal: usize,
        focus_target: Option<usize>,
        document_transition: Option<ViewportTransition>,
    ) -> (Option<ViewportTransition>, bool) {
        let focus = self.commit_ready_focus(requested, mode, focus_target);
        let focused = focus.is_some();
        let navigation = self.commit_ready_navigation(requested, mode, horizontal);
        (focus.or(navigation).or(document_transition), focused)
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
