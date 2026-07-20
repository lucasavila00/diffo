use crate::diff::{
    ChangeTarget, Constraint, DiffViewMode, DiffViewportMetrics, Direction, Layout, Model, Rect,
    Renderer, ScrollbarAxis, design,
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

    pub(in crate::diff) fn hunk_button_direction_at(&self, column: u16, row: u16) -> Option<bool> {
        let position = (column, row).into();
        if self
            .hunk_buttons
            .previous
            .is_some_and(|area| area.contains(position))
        {
            return Some(false);
        }
        self.hunk_buttons
            .next
            .is_some_and(|area| area.contains(position))
            .then_some(true)
    }

    pub(in crate::diff) fn change_at_marker(
        &self,
        column: u16,
        row: u16,
        _model: &Model,
    ) -> Option<usize> {
        let marker_column = self.scrollbars.vertical_area.x.saturating_add(1);
        if column != marker_column {
            return None;
        }
        let cache = self.highlighted.as_ref()?;
        let changes = match cache.key.mode {
            DiffViewMode::Inline => &cache.inline_changes,
            DiffViewMode::SideBySide => &cache.side_by_side_changes,
        };
        changes.iter().map(|change| change.first).find(|change| {
            let marker_row = self
                .scrollbars
                .vertical_area
                .y
                .saturating_add(overview_position(
                    *change,
                    self.scrollbars.rows,
                    self.scrollbars.vertical_area.height,
                ));
            marker_row == row
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
        let inner = area.inner(design::PANEL_INSET);
        let rows = self.displayed_rows(mode);
        let changes = self.change_targets(mode);
        let viewport_columns = usize::from(inner.width);
        let previous_rows = u16::from(inner.height > 0);
        let next_rows = u16::from(inner.height > 1);
        let control_rows = usize::from(previous_rows.saturating_add(next_rows));
        let mut previous_change = None;
        let mut next_change = None;
        let mut show_horizontal = false;
        let mut horizontal_columns = 0;

        for _ in 0..8 {
            let reserved_rows = control_rows + usize::from(show_horizontal);
            let viewport_rows = usize::from(inner.height).saturating_sub(reserved_rows);
            let maximum_vertical_scroll = maximum_scroll(rows, viewport_rows);
            let first_row = requested_scroll.min(maximum_vertical_scroll);
            let new_previous = previous_change_target(changes, first_row, viewport_rows);
            let new_next = next_change_target(changes, first_row, viewport_rows);
            let columns = self.displayed_columns(mode, viewport_columns, first_row, viewport_rows);
            horizontal_columns = horizontal_columns.max(columns);
            let new_horizontal = show_horizontal || columns > viewport_columns;
            if new_previous == previous_change
                && new_next == next_change
                && new_horizontal == show_horizontal
            {
                break;
            }
            previous_change = new_previous;
            next_change = new_next;
            show_horizontal = new_horizontal;
        }

        if previous_rows == 0 {
            previous_change = None;
        }
        if next_rows == 0 {
            next_change = None;
        }
        let horizontal_rows =
            u16::from(show_horizontal && inner.height > previous_rows.saturating_add(next_rows));
        let content_y = inner.y.saturating_add(previous_rows);
        let content_bottom = inner
            .bottom()
            .saturating_sub(horizontal_rows)
            .saturating_sub(next_rows);
        let content_area = Rect::new(
            inner.x,
            content_y,
            inner.width,
            content_bottom.saturating_sub(content_y),
        );
        let horizontal_area = if horizontal_rows == design::SINGLE_LINE_HEIGHT {
            Rect::new(
                inner.x,
                inner.bottom().saturating_sub(design::SINGLE_LINE_HEIGHT),
                inner.width,
                design::SINGLE_LINE_HEIGHT,
            )
        } else {
            Rect::default()
        };
        let viewport_rows = usize::from(content_area.height);
        let maximum_vertical_scroll = maximum_scroll(rows, viewport_rows);
        let first_row = requested_scroll.min(maximum_vertical_scroll);
        let columns = self
            .displayed_columns(mode, viewport_columns, first_row, viewport_rows)
            .max(horizontal_columns);
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
        .find(|change| change.last >= first_below)
        .map(|change| {
            let edge_row = change.first.max(first_below);
            ChangeTarget {
                scroll: edge_row,
                edge_row,
            }
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
        .rev()
        .find(|change| change.first < first_row)
        .map(|change| {
            let scroll = if change.last >= first_row {
                change.first.max(first_row.saturating_sub(viewport_rows))
            } else {
                change.first
            };
            ChangeTarget {
                scroll,
                edge_row: change.last.min(first_row.saturating_sub(1)),
            }
        })
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
        assert_eq!(next_change_target(CHANGES, 1, 8), Some(target(10, 10)));
        assert_eq!(previous_change_target(CHANGES, 4, 8), Some(target(2, 3)));
    }

    #[test]
    fn regions_crossing_viewport_edges_remain_targets() {
        assert_eq!(next_change_target(CHANGES, 4, 3), Some(target(7, 7)));
        assert_eq!(previous_change_target(CHANGES, 7, 3), Some(target(6, 6)));
    }

    #[test]
    fn region_taller_than_the_viewport_moves_one_viewport_at_a_time() {
        assert_eq!(next_change_target(CHANGES, 12, 5), Some(target(17, 17)));
        assert_eq!(previous_change_target(CHANGES, 20, 5), Some(target(15, 19)));
    }

    #[test]
    fn first_and_last_targets_do_not_wrap() {
        assert_eq!(previous_change_target(CHANGES, 0, 5), None);
        assert_eq!(next_change_target(CHANGES, 34, 5), None);
    }
}
