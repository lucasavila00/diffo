use super::{
    Alignment, Block, Borders, Color, DiffViewMode, DiffViewportMetrics, Frame, HunkButtonMetrics,
    HunkDirection, Line, Model, Modifier, Paragraph, Rect, Renderer, Scrollbar, ScrollbarMetrics,
    ScrollbarOrientation, ScrollbarState, Style, inline_line, overview_position,
    resize_border_style, scrollbar_position_count, side_by_side_line,
};

pub(super) fn render_hunk_button(frame: &mut Frame, area: Rect, label: &str, hovered: bool) {
    let style = if hovered {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Yellow).bg(Color::Indexed(235))
    };
    frame.render_widget(
        Paragraph::new(label)
            .alignment(Alignment::Center)
            .style(style),
        area,
    );
}

pub(super) fn render_change_markers(
    frame: &mut Frame,
    area: Rect,
    changes: &[usize],
    rows: usize,
    first_visible: usize,
    viewport_rows: usize,
) {
    for &change in changes {
        let visible =
            change >= first_visible && change < first_visible.saturating_add(viewport_rows);
        let marker = Rect::new(
            area.x.saturating_add(1),
            area.y
                .saturating_add(overview_position(change, rows, area.height)),
            1,
            1,
        );
        frame.render_widget(
            Paragraph::new("▪").style(Style::default().fg(if visible {
                Color::Cyan
            } else {
                Color::Yellow
            })),
            marker,
        );
    }
}

impl Renderer {
    pub(super) fn render_diff(
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
        let lines = self.diff_lines(
            model,
            viewport.content_area.width,
            model.diff_scroll,
            viewport.viewport_rows,
        );
        let resize_label = if model.resizing_file_pane {
            format!(" · files {}%", model.file_pane_percent)
        } else {
            String::new()
        };
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_style(resize_border_style(model))
                .title(format!(" File Diff · {mode}{resize_label} ")),
            area,
        );
        frame.render_widget(
            Paragraph::new(lines).scroll((
                0,
                model.diff_horizontal_scroll.try_into().unwrap_or(u16::MAX),
            )),
            viewport.content_area,
        );
        self.render_hunk_buttons(frame, area, &viewport);
        self.render_diff_scrollbars(frame, area, &viewport, model);
    }

    pub(super) fn render_hunk_buttons(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        viewport: &DiffViewportMetrics,
    ) {
        let inner = area.inner(ratatui::layout::Margin {
            vertical: 1,
            horizontal: 1,
        });
        let previous_area = viewport.previous_change.map(|_| {
            Rect::new(
                inner.x,
                viewport.content_area.y.saturating_sub(1),
                inner.width,
                1,
            )
        });
        let next_area = viewport
            .next_change
            .map(|_| Rect::new(inner.x, viewport.content_area.bottom(), inner.width, 1));
        self.hunk_buttons = HunkButtonMetrics {
            previous: previous_area.zip(viewport.previous_change),
            next: next_area.zip(viewport.next_change),
        };
        if let Some((button, _)) = self.hunk_buttons.previous {
            render_hunk_button(
                frame,
                button,
                "↑ Previous change",
                self.hovered_hunk_button == Some(HunkDirection::Previous),
            );
        }
        if let Some((button, _)) = self.hunk_buttons.next {
            render_hunk_button(
                frame,
                button,
                "↓ Next change",
                self.hovered_hunk_button == Some(HunkDirection::Next),
            );
        }
    }

    pub(super) fn render_diff_scrollbars(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        viewport: &DiffViewportMetrics,
        model: &Model,
    ) {
        self.scrollbars = ScrollbarMetrics {
            vertical_area: Rect::new(
                area.right().saturating_sub(2),
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
        if viewport.maximum_vertical_scroll > 0 {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .thumb_style(Style::default().fg(Color::Cyan));
            let mut state = ScrollbarState::new(viewport.maximum_vertical_scroll.saturating_add(1))
                .viewport_content_length(viewport.viewport_rows)
                .position(model.diff_scroll);
            frame.render_stateful_widget(scrollbar, self.scrollbars.vertical_area, &mut state);
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
        if viewport.columns > viewport.viewport_columns {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
                .begin_symbol(None)
                .end_symbol(None)
                .thumb_style(Style::default().fg(Color::Cyan));
            let mut state = ScrollbarState::new(scrollbar_position_count(
                viewport.columns,
                viewport.viewport_columns,
            ))
            .viewport_content_length(viewport.viewport_columns)
            .position(model.diff_horizontal_scroll);
            frame.render_stateful_widget(scrollbar, self.scrollbars.horizontal_area, &mut state);
        }
    }

    pub(super) fn diff_lines(
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
                .map(|line| Line::raw(line.to_owned()))
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
                let column_width = usize::from(width.saturating_sub(3) / 2);
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
}
