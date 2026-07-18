use super::{
    ChangeArea, Constraint, DiffViewMode, DiffViewportMetrics, Direction, FileKey, FileState,
    Layout, Model, Rect, Renderer, ScrollbarAxis, file_group_areas, file_group_metrics,
    file_panel_areas, staged_files, unstaged_files,
};
pub(super) use diffo_text_view::scrollbar_position;
#[cfg(test)]
pub(super) use diffo_text_view::scrollbar_position_count;

pub(super) fn overview_position(content_row: usize, content_rows: usize, track_height: u16) -> u16 {
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

pub(crate) fn file_at_position(
    model: &Model,
    area: ratatui::layout::Rect,
    column: u16,
    row: u16,
) -> Option<FileKey> {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);
    let columns = horizontal_panes(vertical[0], model.file_pane_percent);
    let file_areas = file_panel_areas(columns[0]);
    let groups = file_group_areas(file_areas[1]);
    let staged_metrics = file_group_metrics(
        groups[0],
        staged_files(&model.snapshot).count(),
        model.file_list_scroll.staged,
    );
    file_in_group_at(
        staged_files(&model.snapshot),
        ChangeArea::Staged,
        staged_metrics,
        column,
        row,
    )
    .or_else(|| {
        let unstaged_metrics = file_group_metrics(
            groups[1],
            unstaged_files(&model.snapshot).count(),
            model.file_list_scroll.unstaged,
        );
        file_in_group_at(
            unstaged_files(&model.snapshot),
            ChangeArea::Unstaged,
            unstaged_metrics,
            column,
            row,
        )
    })
}

pub(crate) fn file_action_at_position(
    model: &Model,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<diffo_app::Message> {
    let columns = horizontal_panes(main_area(area), model.file_pane_percent);
    let file_areas = file_panel_areas(columns[0]);
    let groups = file_group_areas(file_areas[1]);
    if header_action_contains(groups[0], " Staged [", column, row) {
        return Some(diffo_app::Message::UnstageAll);
    }
    if header_action_contains(groups[1], " Changes [", column, row) {
        return Some(diffo_app::Message::StageAll);
    }
    for (group, change_area) in [
        (groups[0], ChangeArea::Staged),
        (groups[1], ChangeArea::Unstaged),
    ] {
        let file_count = match change_area {
            ChangeArea::Staged => staged_files(&model.snapshot).count(),
            ChangeArea::Unstaged => unstaged_files(&model.snapshot).count(),
        };
        let metrics =
            file_group_metrics(group, file_count, model.file_list_scroll.get(change_area));
        let button_start = metrics.list_area.right().saturating_sub(3);
        if column < button_start || column >= metrics.list_area.right() {
            continue;
        }
        let key = match change_area {
            ChangeArea::Staged => file_in_group_at(
                staged_files(&model.snapshot),
                change_area,
                metrics,
                column,
                row,
            ),
            ChangeArea::Unstaged => file_in_group_at(
                unstaged_files(&model.snapshot),
                change_area,
                metrics,
                column,
                row,
            ),
        };
        let Some(key) = key else {
            continue;
        };
        return Some(match change_area {
            ChangeArea::Staged => diffo_app::Message::UnstageFile(key.path),
            ChangeArea::Unstaged => diffo_app::Message::StageFile(key.path),
        });
    }
    None
}

pub(crate) fn file_group_at_position(
    model: &Model,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<ChangeArea> {
    let columns = horizontal_panes(main_area(area), model.file_pane_percent);
    let file_areas = file_panel_areas(columns[0]);
    let groups = file_group_areas(file_areas[1]);
    if groups[0].contains((column, row).into()) {
        Some(ChangeArea::Staged)
    } else if groups[1].contains((column, row).into()) {
        Some(ChangeArea::Unstaged)
    } else {
        None
    }
}

pub(super) fn header_action_contains(area: Rect, prefix: &str, column: u16, row: u16) -> bool {
    let button = area
        .x
        .saturating_add(1)
        .saturating_add(u16::try_from(prefix.chars().count()).unwrap_or(u16::MAX));
    row == area.y && column == button && button < area.right().saturating_sub(1)
}

pub(crate) fn is_file_pane_splitter_at(
    model: &Model,
    area: ratatui::layout::Rect,
    column: u16,
    row: u16,
) -> bool {
    let main = main_area(area);
    if row < main.y || row >= main.y.saturating_add(main.height) {
        return false;
    }
    let panes = horizontal_panes(main, model.file_pane_percent);
    let splitter = panes[1].x;
    column.abs_diff(splitter) <= 1
}

pub(crate) fn file_pane_percent_at(area: ratatui::layout::Rect, column: u16) -> u16 {
    let main = main_area(area);
    if main.width == 0 {
        return 0;
    }
    let offset = column.saturating_sub(main.x).min(main.width);
    u16::try_from(u32::from(offset) * 100 / u32::from(main.width)).unwrap_or(100)
}

pub(super) fn main_area(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area)[0]
}

pub(super) fn horizontal_panes(
    area: ratatui::layout::Rect,
    file_pane_percent: u16,
) -> std::rc::Rc<[ratatui::layout::Rect]> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(file_pane_percent.min(100)),
            Constraint::Percentage(100_u16.saturating_sub(file_pane_percent)),
        ])
        .split(area)
}

pub(super) fn file_in_group_at<'a>(
    mut files: impl Iterator<Item = &'a FileState>,
    change_area: ChangeArea,
    metrics: crate::files::FileGroupMetrics,
    column: u16,
    row: u16,
) -> Option<FileKey> {
    if !metrics.list_area.contains((column, row).into()) {
        return None;
    }
    files
        .nth(
            metrics
                .offset
                .saturating_add(usize::from(row.saturating_sub(metrics.list_area.y))),
        )
        .map(|file| FileKey {
            path: file.path.clone(),
            area: change_area,
        })
}

impl Renderer {
    pub(super) fn file_scrollbar_at(&self, column: u16, row: u16) -> Option<ChangeArea> {
        [ChangeArea::Staged, ChangeArea::Unstaged]
            .into_iter()
            .find(|area| {
                let metrics = self.file_lists.get(*area);
                metrics.maximum_scroll > 0 && metrics.scrollbar_area.contains((column, row).into())
            })
    }

    pub(super) fn file_scrollbar_message(&self, area: ChangeArea, row: u16) -> diffo_app::Message {
        let metrics = self.file_lists.get(area);
        diffo_app::Message::SetFileListScroll(
            area,
            scrollbar_position(
                row.saturating_sub(metrics.scrollbar_area.y),
                metrics.scrollbar_area.height,
                metrics.maximum_scroll,
            ),
        )
    }

    pub(super) fn change_jump(&self, model: &Model, next: bool) -> Option<usize> {
        let cache = self.highlighted.as_ref()?;
        let scroll = model.diff_scroll;
        let changes = match cache.key.mode {
            DiffViewMode::Inline => &cache.inline_changes,
            DiffViewMode::SideBySide => &cache.side_by_side_changes,
        };
        if next {
            changes
                .iter()
                .copied()
                .find(|row| *row > scroll)
                .or_else(|| changes.first().copied())
        } else {
            changes
                .iter()
                .rev()
                .copied()
                .find(|row| *row < scroll)
                .or_else(|| changes.last().copied())
        }
    }

    pub(super) fn hunk_button_target_at(&self, column: u16, row: u16) -> Option<usize> {
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

    pub(super) fn change_at_marker(&self, column: u16, row: u16, _model: &Model) -> Option<usize> {
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

    pub(super) fn scrollbar_at(&self, column: u16, row: u16) -> Option<ScrollbarAxis> {
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

    pub(super) fn scrollbar_message(
        &self,
        axis: ScrollbarAxis,
        column: u16,
        row: u16,
    ) -> diffo_app::Message {
        match axis {
            ScrollbarAxis::Vertical => diffo_app::Message::SetDiffScroll(scrollbar_position(
                row.saturating_sub(self.scrollbars.vertical_area.y),
                self.scrollbars.vertical_area.height,
                self.scrollbars.maximum_vertical_scroll,
            )),
            ScrollbarAxis::Horizontal => {
                diffo_app::Message::SetDiffHorizontalScroll(scrollbar_position(
                    column.saturating_sub(self.scrollbars.horizontal_area.x),
                    self.scrollbars.horizontal_area.width,
                    self.scrollbars
                        .columns
                        .saturating_sub(self.scrollbars.viewport_columns),
                ))
            }
        }
    }

    pub(super) fn diff_viewport_metrics(
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

    pub(super) fn diff_viewport_metrics_at(
        &self,
        mode: DiffViewMode,
        area: Rect,
        requested_scroll: usize,
    ) -> DiffViewportMetrics {
        let inner = area.inner(ratatui::layout::Margin {
            vertical: 1,
            horizontal: 1,
        });
        let rows = self.displayed_rows(mode);
        let changes = self.change_targets(mode);
        let viewport_columns = usize::from(inner.width);
        let mut previous_change = None;
        let mut next_change = None;
        let mut show_horizontal = false;
        let mut horizontal_columns = 0;

        for _ in 0..8 {
            let reserved_rows = usize::from(previous_change.is_some())
                + usize::from(next_change.is_some())
                + usize::from(show_horizontal);
            let viewport_rows = usize::from(inner.height).saturating_sub(reserved_rows);
            let maximum_vertical_scroll = rows.saturating_sub(viewport_rows);
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

        if inner.height < 3 {
            previous_change = None;
            next_change = None;
        }
        let horizontal_rows = u16::from(show_horizontal && inner.height > 1);
        let previous_rows = u16::from(previous_change.is_some());
        let next_rows = u16::from(next_change.is_some());
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
        let horizontal_area = if horizontal_rows == 1 {
            Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1)
        } else {
            Rect::default()
        };
        let viewport_rows = usize::from(content_area.height);
        let maximum_vertical_scroll = rows.saturating_sub(viewport_rows);
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
