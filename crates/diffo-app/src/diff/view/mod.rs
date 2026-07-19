use super::{
    Alignment, Block, Borders, DiffBlock, DiffViewMode, DiffViewportMetrics, Frame,
    HunkButtonMetrics, Line, Model, Paragraph, Rect, Renderer, RowKind, ScrollbarMetrics, Style,
    inline_line, inline_skeleton_line, overview_position, raw_hunk_line, resize_border_style,
    side_by_side_line, side_by_side_skeleton_line, terminal_safe_text,
};
use diffo_ui::text_view::{Viewport, ViewportMetrics, render_lines, render_scrollbars};
use diffo_ui::{design, enabled_control_style, theme};

pub(in crate::diff) mod files;
pub(in crate::diff) mod geometry;
pub(in crate::diff) mod overlays;
pub(in crate::diff) mod style;

pub(in crate::diff) fn render_hunk_button(frame: &mut Frame, area: Rect, label: &str) {
    frame.render_widget(
        Paragraph::new(label)
            .alignment(Alignment::Center)
            .style(enabled_control_style().bg(theme::SELECTION_BACKGROUND)),
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
            Paragraph::new("▪").style(Style::default().fg(if visible {
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
        let metrics = self.full_screen_metrics(area, model.diff_scroll);
        let syntax_ready = self.failed.is_some()
            || self.syntax_ready_for_viewport(
                self.displayed_mode(model.diff_view_mode),
                model.diff_scroll,
            );
        let lines = if syntax_ready {
            self.full_screen_lines(model.diff_scroll, metrics.viewport_rows)
        } else {
            Vec::new()
        };
        render_lines(frame, metrics.area, lines, model.diff_horizontal_scroll);
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
                    cache.inline.len()
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
        let displayed_mode = self.displayed_mode(model.diff_view_mode);
        let mode = match displayed_mode {
            DiffViewMode::Inline => "Inline",
            DiffViewMode::SideBySide => "Side by side",
        };
        let viewport = self.diff_viewport_metrics(displayed_mode, area, model.diff_scroll);
        let skeleton = self.requested.as_ref() == self.displayed_key()
            && !self.syntax_ready_for_viewport(displayed_mode, model.diff_scroll);
        let lines = if skeleton {
            self.diff_skeleton_lines(
                viewport.content_area.width,
                model.diff_scroll,
                viewport.viewport_rows,
            )
        } else {
            self.diff_lines(
                model,
                viewport.content_area.width,
                model.diff_scroll,
                viewport.viewport_rows,
            )
        };
        let title = self
            .displayed_key()
            .map_or_else(|| Line::raw(" File Diff "), |key| key.title.clone());
        let resize_label = if model.resizing_file_pane {
            format!(" · files {}%", model.file_pane_percent)
        } else {
            String::new()
        };
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_style(resize_border_style(model))
                .title(title)
                .title(
                    Line::raw(format!(" {mode}{resize_label} ───  ")).alignment(Alignment::Right),
                ),
            area,
        );
        render_lines(
            frame,
            viewport.content_area,
            lines,
            model.diff_horizontal_scroll,
        );
        self.render_hunk_buttons(frame, area, &viewport);
        self.render_diff_scrollbars(frame, area, &viewport, model);
    }

    pub(in crate::diff) fn render_hunk_buttons(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        viewport: &DiffViewportMetrics,
    ) {
        let inner = area.inner(design::PANEL_INSET);
        let previous_area = viewport.previous_change.map(|_| {
            Rect::new(
                inner.x,
                viewport
                    .content_area
                    .y
                    .saturating_sub(design::SINGLE_LINE_HEIGHT),
                inner.width,
                design::SINGLE_LINE_HEIGHT,
            )
        });
        let next_area = viewport.next_change.map(|_| {
            Rect::new(
                inner.x,
                viewport.content_area.bottom(),
                inner.width,
                design::SINGLE_LINE_HEIGHT,
            )
        });
        self.hunk_buttons = HunkButtonMetrics {
            previous: previous_area,
            next: next_area,
        };
        if let Some(button) = self.hunk_buttons.previous {
            render_hunk_button(frame, button, "↑ Previous change (p)");
        }
        if let Some(button) = self.hunk_buttons.next {
            render_hunk_button(frame, button, "↓ Next change (n)");
        }
    }

    pub(in crate::diff) fn render_diff_scrollbars(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        viewport: &DiffViewportMetrics,
        model: &Model,
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
                vertical: model.diff_scroll,
                horizontal: model.diff_horizontal_scroll,
            },
        );
        debug_assert_eq!(shared.vertical, self.scrollbars.vertical_area);
        if viewport.maximum_vertical_scroll > 0 {
            let changes = self.highlighted.as_ref().map(|cache| match cache.key.mode {
                DiffViewMode::Inline => cache.inline_changes.as_slice(),
                DiffViewMode::SideBySide => cache.side_by_side_changes.as_slice(),
            });
            if let Some(changes) = changes {
                render_change_markers(
                    frame,
                    self.scrollbars.vertical_area,
                    changes,
                    viewport.rows,
                    model.diff_scroll,
                    viewport.viewport_rows,
                );
            }
        }
    }

    pub(in crate::diff) fn diff_lines(
        &self,
        model: &Model,
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
            if model.selected.is_some() {
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
                    .map(|row| side_by_side_line(row, column_width, &cache.highlighted))
                    .collect()
            }
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
        }
    }
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
