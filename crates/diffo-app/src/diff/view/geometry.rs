use crate::diff::{
    Constraint, DiffViewMode, DiffViewportMetrics, Direction, Layout, Model, Rect, Renderer,
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
    pub(in crate::diff) fn change_jump(&self, model: &Model, next: bool) -> Option<usize> {
        let cache = self.highlighted.as_ref()?;
        let scroll = model.diff_scroll;
        let changes = match cache.key.mode {
            DiffViewMode::Inline => &cache.inline_changes,
            DiffViewMode::SideBySide => &cache.side_by_side_changes,
        };
        if next {
            changes.iter().copied().find(|row| *row > scroll)
        } else {
            changes.iter().rev().copied().find(|row| *row < scroll)
        }
    }

    pub(in crate::diff) fn hunk_button_target_at(&self, column: u16, row: u16) -> Option<usize> {
        let position = (column, row).into();
        if let Some((area, target)) = self.hunk_buttons.previous
            && area.contains(position)
        {
            return Some(target);
        }
        self.hunk_buttons
            .next
            .filter(|(area, _)| area.contains(position))
            .map(|(_, target)| target)
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
        changes.iter().copied().find(|change| {
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
            let new_previous = changes.iter().rev().copied().find(|row| *row < first_row);
            let new_next = changes
                .iter()
                .copied()
                .find(|row| *row >= first_row.saturating_add(viewport_rows));
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
