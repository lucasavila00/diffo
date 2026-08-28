use super::{
    Alignment, Block, Borders, ChangeWarningAreas, Clear, DiffBlock, DiffViewMode,
    DiffViewportMetrics, Frame, Line, Model, Paragraph, Rect, Renderer, RowKind, ScrollbarMetrics,
    Style, inline_line, inline_skeleton_line, overview_position, raw_hunk_line,
    resize_border_style, side_by_side_line, side_by_side_skeleton_line, terminal_safe_text,
};
use diffo_ui::text_view::{Viewport, ViewportMetrics, render_lines, render_scrollbars};
use diffo_ui::{design, icons, mouse_target_style, theme};

pub(in crate::diff) mod files;
pub(in crate::diff) mod geometry;
pub(in crate::diff) mod overlays;
pub(in crate::diff) mod style;

#[derive(Clone, Copy)]
pub(crate) struct ReviewRender<'a> {
    pub(crate) mode: DiffViewMode,
    pub(crate) vertical: usize,
    pub(crate) horizontal: usize,
    pub(crate) border_style: Style,
    pub(crate) trailing_title: &'a str,
    pub(crate) has_selection: bool,
    pub(crate) empty_title: &'static str,
}

pub(in crate::diff) fn render_change_warning(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    background: Style,
) {
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(label)
            .alignment(Alignment::Center)
            .style(mouse_target_style().patch(background)),
        area,
    );
}

pub(in crate::diff) fn render_change_markers(
    frame: &mut Frame,
    area: Rect,
    changes: &[crate::diff::ChangeRegion],
    rows: usize,
    first_visible: usize,
    viewport_rows: usize,
) {
    for change in changes.iter().map(|change| change.first) {
        let visible =
            change >= first_visible && change < first_visible.saturating_add(viewport_rows);
        let marker = Rect::new(
            area.x.saturating_add(design::BORDER_WIDTH),
            area.y
                .saturating_add(overview_position(change, rows, area.height)),
            1,
            1,
        );
        frame.render_widget(
            Paragraph::new(icons::CHANGE_MARKER).style(Style::default().fg(if visible {
                theme::TEXT
            } else {
                theme::CHROME
            })),
            marker,
        );
    }
}

impl Renderer {
    pub fn render_full_screen(&mut self, frame: &mut Frame, area: Rect, model: &Model) {
        self.render_review_full_screen(
            frame,
            area,
            model.diff_view_mode,
            model.diff_scroll,
            model.diff_horizontal_scroll,
        );
    }

    pub(crate) fn render_review_full_screen(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        requested_mode: DiffViewMode,
        vertical: usize,
        horizontal: usize,
    ) {
        let metrics = self.full_screen_metrics(area, vertical);
        let syntax_ready = self.failed.is_some()
            || self.syntax_ready_for_viewport(self.displayed_mode(requested_mode), vertical);
        let lines = if syntax_ready {
            self.full_screen_lines(vertical, metrics.viewport_rows)
        } else {
            Vec::new()
        };
        render_lines(frame, metrics.area, lines, horizontal);
        self.scrollbar_drag = None;
        self.scrollbars = ScrollbarMetrics {
            rows: metrics.rows,
            columns: metrics.columns,
            viewport_columns: metrics.viewport_columns,
            maximum_vertical_scroll: metrics.maximum_vertical,
            ..ScrollbarMetrics::default()
        };
    }

    pub(in crate::diff) fn full_screen_metrics(
        &self,
        area: Rect,
        requested_scroll: usize,
    ) -> ViewportMetrics {
        let rows = self.full_screen_rows();
        let viewport_rows = usize::from(area.height);
        let viewport_columns = usize::from(area.width);
        let maximum_vertical = diffo_ui::maximum_scroll(rows, viewport_rows);
        let first = requested_scroll.min(maximum_vertical);
        let columns = self
            .full_screen_lines(first, viewport_rows)
            .iter()
            .map(Line::width)
            .max()
            .unwrap_or(0);
        ViewportMetrics {
            area,
            horizontal_scrollbar: Rect::default(),
            rows,
            columns,
            viewport_rows,
            viewport_columns,
            maximum_vertical,
            maximum_horizontal: diffo_ui::maximum_scroll(columns, viewport_columns),
        }
    }

    fn full_screen_rows(&self) -> usize {
        self.highlighted.as_ref().map_or_else(
            || {
                self.failed
                    .as_ref()
                    .map_or(0, |key| key.patch.lines().count())
            },
            |cache| {
                if cache.document.binary {
                    1
                } else {
                    cache.hunk.len()
                }
            },
        )
    }

    fn full_screen_lines(&self, first_row: usize, row_count: usize) -> Vec<Line<'static>> {
        if let Some(failed) = self.failed.as_ref() {
            return failed
                .patch
                .lines()
                .skip(first_row)
                .take(row_count)
                .map(|line| Line::raw(terminal_safe_text(line)))
                .collect();
        }
        let Some(cache) = self.highlighted.as_ref() else {
            return Vec::new();
        };
        if cache.document.binary {
            return vec![Line::raw("Binary file changed.")];
        }
        if cache.key.selection.complete_change_id().is_some() {
            return cache
                .hunk
                .iter()
                .skip(first_row)
                .take(row_count)
                .map(|row| hunk_line(row, &cache.highlighted))
                .collect();
        }
        let end = first_row.saturating_add(row_count);
        let mut index = 0_usize;
        let mut lines = Vec::new();
        let mut push = |line: Line<'static>| {
            if index >= first_row && index < end {
                lines.push(line);
            }
            index = index.saturating_add(1);
        };
        for hunk in &cache.document.hunks {
            push(raw_hunk_line(None, &hunk.header, RowKind::Header, None));
            for block in &hunk.blocks {
                match block {
                    DiffBlock::Context(rows) => {
                        for row in rows {
                            let highlighted = row
                                .new_number
                                .and_then(|number| cache.highlighted.new.get(&number));
                            push(raw_hunk_line(
                                Some(' '),
                                &row.text,
                                raw_hunk_kind(
                                    &row.text,
                                    RowKind::Context,
                                    cache.key.mark_conflicts,
                                ),
                                highlighted,
                            ));
                        }
                    }
                    DiffBlock::Change { removed, added, .. } => {
                        for row in removed {
                            let highlighted = row
                                .old_number
                                .and_then(|number| cache.highlighted.old.get(&number));
                            push(raw_hunk_line(
                                Some('-'),
                                &row.text,
                                raw_hunk_kind(
                                    &row.text,
                                    RowKind::Removed,
                                    cache.key.mark_conflicts,
                                ),
                                highlighted,
                            ));
                        }
                        for row in added {
                            let highlighted = row
                                .new_number
                                .and_then(|number| cache.highlighted.new.get(&number));
                            push(raw_hunk_line(
                                Some('+'),
                                &row.text,
                                raw_hunk_kind(&row.text, RowKind::Added, cache.key.mark_conflicts),
                                highlighted,
                            ));
                        }
                    }
                    DiffBlock::Meta(text) => {
                        push(raw_hunk_line(None, text, RowKind::Meta, None));
                    }
                }
            }
        }
        lines
    }

    pub(in crate::diff) fn render_diff(
        &mut self,
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        model: &Model,
    ) {
        let resize_label = if model.resizing_file_pane {
            format!(" · files {}%", model.file_pane_percent)
        } else {
            String::new()
        };
        self.render_review(
            frame,
            area,
            ReviewRender {
                mode: model.diff_view_mode,
                vertical: model.diff_scroll,
                horizontal: model.diff_horizontal_scroll,
                border_style: resize_border_style(model),
                trailing_title: &resize_label,
                has_selection: model.selected.is_some(),
                empty_title: "File Diff",
            },
        );
    }

    pub(crate) fn render_review(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        review: ReviewRender<'_>,
    ) {
        let ReviewRender {
            mode: requested_mode,
            vertical,
            horizontal,
            border_style,
            trailing_title,
            has_selection,
            empty_title,
        } = review;
        let displayed_mode = self.displayed_mode(requested_mode);
        let mode = match displayed_mode {
            DiffViewMode::Inline => "Inline",
            DiffViewMode::SideBySide => "Side by side",
            DiffViewMode::Hunk => "Hunk",
        };
        let viewport = self.diff_viewport_metrics(displayed_mode, area, vertical);
        let skeleton = self.requested.as_ref() == self.displayed_key()
            && !self.syntax_ready_for_viewport(displayed_mode, vertical);
        let lines = if skeleton {
            self.diff_skeleton_lines(
                viewport.content_area.width,
                vertical,
                viewport.viewport_rows,
            )
        } else {
            self.review_lines(
                has_selection,
                horizontal,
                viewport.content_area.width,
                vertical,
                viewport.viewport_rows,
            )
        };
        let title = self.displayed_key().map_or_else(
            || Line::raw(format!(" {empty_title} ")),
            |key| key.title.clone(),
        );
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(title)
                .title(
                    Line::raw(format!(" {mode}{trailing_title} ───  ")).alignment(Alignment::Right),
                ),
            area,
        );
        render_lines(
            frame,
            viewport.content_area,
            lines,
            if displayed_mode == DiffViewMode::SideBySide {
                0
            } else {
                horizontal
            },
        );
        self.render_change_warnings(frame, &viewport);
        self.render_diff_scrollbars(frame, area, &viewport, vertical, horizontal);
    }

    pub(in crate::diff) fn render_change_warnings(
        &mut self,
        frame: &mut Frame,
        viewport: &DiffViewportMetrics,
    ) {
        self.change_warnings = ChangeWarningAreas::default();
        if viewport.content_area.is_empty() {
            return;
        }

        let warning_width = viewport
            .content_area
            .width
            .saturating_sub(design::DIFF_RIGHT_RAIL_WIDTH);
        let top = Rect::new(
            viewport.content_area.x,
            viewport.content_area.y,
            warning_width,
            design::SINGLE_LINE_HEIGHT,
        );
        let bottom = Rect::new(
            viewport.content_area.x,
            viewport
                .content_area
                .bottom()
                .saturating_sub(design::SINGLE_LINE_HEIGHT),
            warning_width,
            design::SINGLE_LINE_HEIGHT,
        );

        if top == bottom
            && let (Some(previous), Some(next)) = (viewport.previous_change, viewport.next_change)
        {
            let previous_width = top.width / 2;
            let previous_area = Rect::new(top.x, top.y, previous_width, top.height);
            let next_area = Rect::new(
                top.x.saturating_add(previous_width),
                top.y,
                top.width.saturating_sub(previous_width),
                top.height,
            );
            self.change_warnings = ChangeWarningAreas {
                previous: Some(previous_area),
                next: Some(next_area),
            };
            render_change_warning(
                frame,
                previous_area,
                &format!("{} p", icons::CHANGE_PREVIOUS),
                self.change_navigation_background(previous.edge_row, false),
            );
            render_change_warning(
                frame,
                next_area,
                &format!("n {}", icons::CHANGE_NEXT),
                self.change_navigation_background(next.edge_row, true),
            );
            return;
        }

        if let Some(target) = viewport.previous_change {
            self.change_warnings.previous = Some(top);
            render_change_warning(
                frame,
                top,
                &format!("{} Previous change (p)", icons::CHANGE_PREVIOUS),
                self.change_navigation_background(target.edge_row, false),
            );
        }
        if let Some(target) = viewport.next_change {
            self.change_warnings.next = Some(bottom);
            render_change_warning(
                frame,
                bottom,
                &format!("{} Next change (n)", icons::CHANGE_NEXT),
                self.change_navigation_background(target.edge_row, true),
            );
        }
    }

    pub(in crate::diff) fn change_navigation_background(&self, row: usize, next: bool) -> Style {
        let kind = self
            .highlighted
            .as_ref()
            .and_then(|cache| match cache.key.mode {
                DiffViewMode::Inline => cache.inline.get(row).map(|row| row.kind),
                DiffViewMode::SideBySide => cache.side_by_side.get(row).and_then(|row| {
                    let (primary, fallback) = if next {
                        (row.new.as_ref(), row.old.as_ref())
                    } else {
                        (row.old.as_ref(), row.new.as_ref())
                    };
                    primary.or(fallback).map(|line| line.kind)
                }),
                DiffViewMode::Hunk => cache.hunk.get(row).map(|row| row.kind),
            });
        match kind {
            Some(kind @ (RowKind::Added | RowKind::Removed | RowKind::Conflict)) => {
                style::diff_background(kind)
            }
            _ => Style::default(),
        }
    }

    pub(in crate::diff) fn render_diff_scrollbars(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        viewport: &DiffViewportMetrics,
        vertical: usize,
        horizontal: usize,
    ) {
        self.scrollbars = ScrollbarMetrics {
            vertical_area: Rect::new(
                area.right().saturating_sub(design::DIFF_RIGHT_RAIL_WIDTH),
                viewport.content_area.y,
                u16::from(area.width > 2),
                viewport.content_area.height,
            ),
            horizontal_area: viewport.horizontal_area,
            rows: viewport.rows,
            columns: viewport.columns,
            viewport_columns: viewport.viewport_columns,
            maximum_vertical_scroll: viewport.maximum_vertical_scroll,
        };
        let shared = render_scrollbars(
            frame,
            Rect::new(
                area.x,
                area.y,
                area.width.saturating_sub(design::BORDER_WIDTH),
                area.height,
            ),
            ViewportMetrics {
                area: viewport.content_area,
                horizontal_scrollbar: viewport.horizontal_area,
                rows: viewport.rows,
                columns: viewport.columns,
                viewport_rows: viewport.viewport_rows,
                viewport_columns: viewport.viewport_columns,
                maximum_vertical: viewport.maximum_vertical_scroll,
                maximum_horizontal: diffo_ui::maximum_scroll(
                    viewport.columns,
                    viewport.viewport_columns,
                ),
            },
            Viewport {
                vertical,
                horizontal,
            },
        );
        debug_assert_eq!(shared.vertical, self.scrollbars.vertical_area);
        if viewport.maximum_vertical_scroll > 0 {
            let changes = self.highlighted.as_ref().map(|cache| match cache.key.mode {
                DiffViewMode::Inline => cache.inline_changes.as_slice(),
                DiffViewMode::SideBySide => cache.side_by_side_changes.as_slice(),
                DiffViewMode::Hunk => cache.hunk_changes.as_slice(),
            });
            if let Some(changes) = changes {
                render_change_markers(
                    frame,
                    self.scrollbars.vertical_area,
                    changes,
                    viewport.rows,
                    vertical,
                    viewport.viewport_rows,
                );
            }
        }
    }

    #[cfg(test)]
    pub(in crate::diff) fn diff_lines(
        &self,
        model: &Model,
        width: u16,
        first_row: usize,
        row_count: usize,
    ) -> Vec<Line<'static>> {
        self.review_lines(
            model.selected.is_some(),
            model.diff_horizontal_scroll,
            width,
            first_row,
            row_count,
        )
    }

    fn review_lines(
        &self,
        has_selection: bool,
        horizontal: usize,
        width: u16,
        first_row: usize,
        row_count: usize,
    ) -> Vec<Line<'static>> {
        if let Some(failed) = self.failed.as_ref() {
            return failed
                .patch
                .lines()
                .skip(first_row)
                .take(row_count)
                .map(|line| Line::raw(terminal_safe_text(line)))
                .collect();
        }
        let Some(cache) = self.highlighted.as_ref() else {
            if has_selection {
                return Vec::new();
            }
            return vec![Line::raw("No file selected.")];
        };
        if cache.document.binary {
            return vec![Line::raw("Binary file changed.")];
        }

        match cache.key.mode {
            DiffViewMode::Inline => cache
                .inline
                .iter()
                .skip(first_row)
                .take(row_count)
                .map(|row| inline_line(row, &cache.highlighted, usize::from(width)))
                .collect(),
            DiffViewMode::SideBySide => {
                let column_width = usize::from(
                    width.saturating_sub(design::SIDE_BY_SIDE_DIVIDER_WIDTH)
                        / design::SIDE_BY_SIDE_COLUMN_COUNT,
                );
                cache
                    .side_by_side
                    .iter()
                    .skip(first_row)
                    .take(row_count)
                    .map(|row| side_by_side_line(row, column_width, horizontal, &cache.highlighted))
                    .collect()
            }
            DiffViewMode::Hunk => cache
                .hunk
                .iter()
                .skip(first_row)
                .take(row_count)
                .map(|row| hunk_line(row, &cache.highlighted))
                .collect(),
        }
    }

    pub(in crate::diff) fn diff_skeleton_lines(
        &self,
        width: u16,
        first_row: usize,
        row_count: usize,
    ) -> Vec<Line<'static>> {
        let Some(cache) = self.highlighted.as_ref() else {
            return Vec::new();
        };
        match cache.key.mode {
            DiffViewMode::Inline => cache
                .inline
                .iter()
                .skip(first_row)
                .take(row_count)
                .map(inline_skeleton_line)
                .collect(),
            DiffViewMode::SideBySide => {
                let column_width = usize::from(
                    width.saturating_sub(design::SIDE_BY_SIDE_DIVIDER_WIDTH)
                        / design::SIDE_BY_SIDE_COLUMN_COUNT,
                );
                cache
                    .side_by_side
                    .iter()
                    .skip(first_row)
                    .take(row_count)
                    .map(|row| side_by_side_skeleton_line(row, column_width))
                    .collect()
            }
            DiffViewMode::Hunk => cache
                .hunk
                .iter()
                .skip(first_row)
                .take(row_count)
                .map(|row| raw_hunk_line(row.prefix, &row.text, row.kind, None))
                .collect(),
        }
    }
}

fn hunk_line(row: &super::HunkRow, highlighted: &super::HighlightedDiff) -> Line<'static> {
    let syntax = match row.kind {
        RowKind::Removed => row
            .old_number
            .and_then(|number| highlighted.old.get(&number)),
        RowKind::Added | RowKind::Context | RowKind::Changed => row
            .new_number
            .and_then(|number| highlighted.new.get(&number)),
        RowKind::Header | RowKind::Conflict | RowKind::Meta => None,
    };
    raw_hunk_line(row.prefix, &row.text, row.kind, syntax)
}

fn raw_hunk_kind(text: &str, fallback: RowKind, mark_conflicts: bool) -> RowKind {
    if mark_conflicts
        && (text.starts_with("<<<<<<<")
            || text.starts_with("|||||||")
            || text.starts_with("=======")
            || text.starts_with(">>>>>>>"))
    {
        RowKind::Conflict
    } else {
        fallback
    }
}
