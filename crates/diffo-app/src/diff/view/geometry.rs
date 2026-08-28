#[cfg(test)]
use crate::diff::Model;
use crate::diff::{
    ChangeTarget, Constraint, DiffViewMode, DiffViewportMetrics, Direction, Layout, Rect, Renderer,
    ScrollbarAxis, design,
};
#[cfg(test)]
pub(in crate::diff) use diffo_ui::scrollbar_position_count;
pub(in crate::diff) use diffo_ui::{maximum_scroll, scrollbar_position};

pub(in crate::diff) fn overview_position(
    content_row: usize,
    content_rows: usize,
    track_height: u16,
) -> u16 {
    if track_height <= 1 || content_rows <= 1 {
        return 0;
    }
    let last_track_row = usize::from(track_height - 1);
    let position = content_row
        .min(content_rows - 1)
        .saturating_mul(last_track_row)
        / (content_rows - 1);
    u16::try_from(position).unwrap_or(track_height - 1)
}

pub(in crate::diff) fn diff_panel_inner(area: Rect) -> Rect {
    Rect::new(
        area.x.saturating_add(design::BORDER_WIDTH.min(area.width)),
        area.y.saturating_add(design::BORDER_WIDTH.min(area.height)),
        design::panel_content_extent(area.width),
        design::panel_content_extent(area.height),
    )
}

pub(in crate::diff) fn main_area(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(design::MIN_TOOL_CONTENT_HEIGHT),
            Constraint::Length(design::STATUS_HEIGHT),
        ])
        .split(area)[0]
}

pub(in crate::diff) fn horizontal_panes(
    area: ratatui::layout::Rect,
    file_pane_percent: u16,
) -> std::rc::Rc<[ratatui::layout::Rect]> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(file_pane_percent.min(design::FULL_PERCENT)),
            Constraint::Percentage(design::FULL_PERCENT.saturating_sub(file_pane_percent)),
        ])
        .split(area)
}

impl Renderer {
    pub(crate) fn review_change_jump(
        &self,
        mode: DiffViewMode,
        area: Rect,
        scroll: usize,
        next: bool,
    ) -> Option<usize> {
        let viewport = self.diff_viewport_metrics(self.displayed_mode(mode), area, scroll);
        if next {
            viewport.next_change.map(|target| target.scroll)
        } else {
            viewport.previous_change.map(|target| target.scroll)
        }
    }

    #[cfg(test)]
    pub(in crate::diff) fn change_jump(
        &self,
        model: &Model,
        area: Rect,
        next: bool,
    ) -> Option<usize> {
        let mode = self.displayed_mode(model.diff_view_mode);
        let diff_area = horizontal_panes(main_area(area), model.file_pane_percent)[1];
        let viewport = self.diff_viewport_metrics(mode, diff_area, model.diff_scroll);
        if next {
            viewport.next_change.map(|target| target.scroll)
        } else {
            viewport.previous_change.map(|target| target.scroll)
        }
    }

    pub(in crate::diff) fn change_at_marker(&self, column: u16, row: u16) -> Option<usize> {
        let marker_column = self.scrollbars.vertical_area.x.saturating_add(1);
        if column != marker_column {
            return None;
        }
        let cache = self.highlighted.as_ref()?;
        let changes = match cache.key.mode {
            DiffViewMode::Inline => &cache.inline_changes,
            DiffViewMode::SideBySide => &cache.side_by_side_changes,
            DiffViewMode::Hunk => &cache.hunk_changes,
        };
        changes.iter().enumerate().find_map(|(index, change)| {
            let marker_row = self
                .scrollbars
                .vertical_area
                .y
                .saturating_add(overview_position(
                    change.first,
                    self.scrollbars.rows,
                    self.scrollbars.vertical_area.height,
                ));
            (marker_row == row).then(|| change_scroll(changes, index))
        })
    }

    pub(in crate::diff) fn scrollbar_at(&self, column: u16, row: u16) -> Option<ScrollbarAxis> {
        if self.scrollbars.maximum_vertical_scroll > 0
            && self.scrollbars.vertical_area.contains((column, row).into())
        {
            Some(ScrollbarAxis::Vertical)
        } else if self.scrollbars.columns > self.scrollbars.viewport_columns
            && self
                .scrollbars
                .horizontal_area
                .contains((column, row).into())
        {
            Some(ScrollbarAxis::Horizontal)
        } else {
            None
        }
    }

    pub(in crate::diff) fn scrollbar_message(
        &self,
        axis: ScrollbarAxis,
        column: u16,
        row: u16,
    ) -> crate::diff::Message {
        match axis {
            ScrollbarAxis::Vertical => crate::diff::Message::SetDiffScroll(scrollbar_position(
                row.saturating_sub(self.scrollbars.vertical_area.y),
                self.scrollbars.vertical_area.height,
                self.scrollbars.maximum_vertical_scroll,
            )),
            ScrollbarAxis::Horizontal => {
                crate::diff::Message::SetDiffHorizontalScroll(scrollbar_position(
                    column.saturating_sub(self.scrollbars.horizontal_area.x),
                    self.scrollbars.horizontal_area.width,
                    maximum_scroll(self.scrollbars.columns, self.scrollbars.viewport_columns),
                ))
            }
        }
    }

    pub(in crate::diff) fn diff_viewport_metrics(
        &self,
        mode: DiffViewMode,
        area: Rect,
        requested_scroll: usize,
    ) -> DiffViewportMetrics {
        let mut viewport = self.diff_viewport_metrics_at(mode, area, requested_scroll);
        viewport.maximum_vertical_scroll = self
            .diff_viewport_metrics_at(mode, area, usize::MAX)
            .maximum_vertical_scroll;
        viewport
    }

    pub(in crate::diff) fn diff_viewport_metrics_at(
        &self,
        mode: DiffViewMode,
        area: Rect,
        requested_scroll: usize,
    ) -> DiffViewportMetrics {
        let inner = diff_panel_inner(area);
        let rows = self.displayed_rows(mode);
        let changes = self.change_targets(mode);
        let viewport_columns = if mode == DiffViewMode::SideBySide {
            usize::from(
                inner
                    .width
                    .saturating_sub(design::SIDE_BY_SIDE_DIVIDER_WIDTH)
                    / design::SIDE_BY_SIDE_COLUMN_COUNT,
            )
            .saturating_sub(usize::from(design::SIDE_BY_SIDE_GUTTER_WIDTH))
        } else {
            usize::from(inner.width)
        };
        let content_area = inner;
        let viewport_rows = usize::from(content_area.height);
        let maximum_vertical_scroll = maximum_scroll(rows, viewport_rows);
        let first_row = requested_scroll.min(maximum_vertical_scroll);
        let previous_change = previous_change_target(changes, first_row, viewport_rows);
        let next_change = next_change_target(changes, first_row, viewport_rows);
        let columns = self.displayed_columns(mode, first_row, viewport_rows);
        let show_horizontal = viewport_columns > 0 && columns > viewport_columns;
        let horizontal_area = if show_horizontal && !inner.is_empty() {
            Rect::new(
                inner.x,
                inner.bottom(),
                inner.width,
                design::SINGLE_LINE_HEIGHT,
            )
        } else {
            Rect::default()
        };
        DiffViewportMetrics {
            content_area,
            horizontal_area,
            viewport_rows,
            viewport_columns,
            rows,
            columns,
            maximum_vertical_scroll,
            previous_change,
            next_change,
        }
    }
}

fn next_change_target(
    changes: &[crate::diff::ChangeRegion],
    first_row: usize,
    viewport_rows: usize,
) -> Option<ChangeTarget> {
    if viewport_rows == 0 {
        return None;
    }
    let first_below = first_row.saturating_add(viewport_rows);
    changes
        .iter()
        .enumerate()
        .find(|(_, change)| change.first >= first_below)
        .map(|(index, change)| ChangeTarget {
            scroll: change_scroll(changes, index),
            edge_row: change.first,
        })
}

fn previous_change_target(
    changes: &[crate::diff::ChangeRegion],
    first_row: usize,
    viewport_rows: usize,
) -> Option<ChangeTarget> {
    if viewport_rows == 0 {
        return None;
    }
    changes
        .iter()
        .enumerate()
        .rev()
        .find(|(_, change)| change.last < first_row)
        .map(|(index, change)| ChangeTarget {
            scroll: change_scroll(changes, index),
            edge_row: change.last,
        })
}

fn change_scroll(changes: &[crate::diff::ChangeRegion], index: usize) -> usize {
    changes[index].first.saturating_sub(usize::from(index > 0))
}

#[cfg(test)]
mod tests {
    use super::{next_change_target, previous_change_target};
    use crate::diff::{ChangeRegion, ChangeTarget};

    const CHANGES: &[ChangeRegion] = &[
        ChangeRegion { first: 2, last: 3 },
        ChangeRegion { first: 6, last: 7 },
        ChangeRegion {
            first: 10,
            last: 30,
        },
        ChangeRegion {
            first: 34,
            last: 35,
        },
    ];

    const fn target(scroll: usize, edge_row: usize) -> ChangeTarget {
        ChangeTarget { scroll, edge_row }
    }

    #[test]
    fn skips_fully_visible_changes_in_both_directions() {
        assert_eq!(next_change_target(CHANGES, 1, 8), Some(target(9, 10)));
        assert_eq!(previous_change_target(CHANGES, 4, 8), Some(target(2, 3)));
    }

    #[test]
    fn skips_regions_crossing_viewport_edges() {
        assert_eq!(next_change_target(CHANGES, 4, 3), Some(target(9, 10)));
        assert_eq!(previous_change_target(CHANGES, 7, 3), Some(target(2, 3)));
    }

    #[test]
    fn skips_a_region_taller_than_the_viewport() {
        assert_eq!(next_change_target(CHANGES, 12, 5), Some(target(33, 34)));
        assert_eq!(previous_change_target(CHANGES, 20, 5), Some(target(5, 7)));
    }

    #[test]
    fn first_and_last_targets_do_not_wrap() {
        assert_eq!(previous_change_target(CHANGES, 0, 5), None);
        assert_eq!(next_change_target(CHANGES, 34, 5), None);
    }
}
