#![doc = include_str!("../README.md")]

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use diffo_ui::{
    design, maximum_scroll, scroll_offset, scrollbar_position, scrollbar_position_count, theme,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextSurface {
    Diff,
    Explorer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextRenderMode {
    Full,
    SyntaxSkeleton,
    TextSkeleton,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextSurfacePreparation {
    pub surface: TextSurface,
    pub document_revision: u64,
    pub viewport: (usize, usize),
    pub requested_range: (usize, usize),
    pub mode: TextRenderMode,
    pub coverage_before: Option<(u32, u32)>,
    pub coverage_after: Option<(u32, u32)>,
    pub request_id: Option<u64>,
    pub cache_hit: bool,
    pub coalesced_request: bool,
    pub stale_discarded: bool,
}

pub const LINE_SCROLL_ROWS: i64 = 4;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Viewport {
    pub vertical: usize,
    pub horizontal: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollCommand {
    Lines(i64),
    Page(i64),
    Columns(i64),
    Vertical(usize),
    Horizontal(usize),
    Home,
    End,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ViewportMetrics {
    pub area: Rect,
    pub horizontal_scrollbar: Rect,
    pub rows: usize,
    pub columns: usize,
    pub viewport_rows: usize,
    pub viewport_columns: usize,
    pub maximum_vertical: usize,
    pub maximum_horizontal: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScrollbarAreas {
    pub vertical: Rect,
    pub horizontal: Rect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollbarAxis {
    Vertical,
    Horizontal,
}

#[must_use]
pub fn scrollbar_areas(outer: Rect, metrics: ViewportMetrics) -> ScrollbarAreas {
    ScrollbarAreas {
        vertical: Rect::new(
            outer.right().saturating_sub(design::BORDER_WIDTH),
            metrics.area.y,
            design::BORDER_WIDTH.min(outer.width),
            metrics.area.height,
        ),
        horizontal: metrics.horizontal_scrollbar,
    }
}

#[must_use]
pub fn scrollbar_axis_at(
    areas: ScrollbarAreas,
    metrics: ViewportMetrics,
    column: u16,
    row: u16,
) -> Option<ScrollbarAxis> {
    if metrics.maximum_vertical > 0 && areas.vertical.contains((column, row).into()) {
        Some(ScrollbarAxis::Vertical)
    } else if metrics.maximum_horizontal > 0 && areas.horizontal.contains((column, row).into()) {
        Some(ScrollbarAxis::Horizontal)
    } else {
        None
    }
}

#[must_use]
pub fn scrollbar_command(
    axis: ScrollbarAxis,
    areas: ScrollbarAreas,
    metrics: ViewportMetrics,
    column: u16,
    row: u16,
) -> ScrollCommand {
    match axis {
        ScrollbarAxis::Vertical => ScrollCommand::Vertical(scrollbar_position(
            row.saturating_sub(areas.vertical.y),
            areas.vertical.height,
            metrics.maximum_vertical,
        )),
        ScrollbarAxis::Horizontal => ScrollCommand::Horizontal(scrollbar_position(
            column.saturating_sub(areas.horizontal.x),
            areas.horizontal.width,
            metrics.maximum_horizontal,
        )),
    }
}

pub fn render_scrollbars(
    frame: &mut Frame,
    outer: Rect,
    metrics: ViewportMetrics,
    viewport: Viewport,
) -> ScrollbarAreas {
    let areas = scrollbar_areas(outer, metrics);
    let style = Style::default().fg(theme::CHROME);
    if metrics.maximum_vertical > 0 {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_style(style)
            .thumb_style(style);
        let mut state = ScrollbarState::new(metrics.maximum_vertical.saturating_add(1))
            .viewport_content_length(metrics.viewport_rows)
            .position(viewport.vertical);
        frame.render_stateful_widget(scrollbar, areas.vertical, &mut state);
    }
    if metrics.maximum_horizontal > 0 {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
            .begin_symbol(None)
            .end_symbol(None)
            .track_style(style)
            .thumb_style(style);
        let mut state = ScrollbarState::new(scrollbar_position_count(
            metrics.columns,
            metrics.viewport_columns,
        ))
        .viewport_content_length(metrics.viewport_columns)
        .position(viewport.horizontal);
        frame.render_stateful_widget(scrollbar, areas.horizontal, &mut state);
    }
    areas
}

pub fn render_lines(frame: &mut Frame, area: Rect, lines: Vec<Line<'static>>, horizontal: usize) {
    frame.render_widget(
        Paragraph::new(lines).scroll((0, horizontal.try_into().unwrap_or(u16::MAX))),
        area,
    );
}

impl Viewport {
    pub fn apply(&mut self, command: ScrollCommand, metrics: ViewportMetrics) {
        match command {
            ScrollCommand::Lines(pages) => {
                self.vertical = scroll_offset(self.vertical, pages, metrics.maximum_vertical);
            }
            ScrollCommand::Page(pages) => {
                let rows = i64::try_from(metrics.viewport_rows).unwrap_or(i64::MAX);
                self.vertical = scroll_offset(
                    self.vertical,
                    pages.saturating_mul(rows),
                    metrics.maximum_vertical,
                );
            }
            ScrollCommand::Columns(columns) => {
                self.horizontal =
                    scroll_offset(self.horizontal, columns, metrics.maximum_horizontal);
            }
            ScrollCommand::Vertical(position) => self.vertical = position,
            ScrollCommand::Horizontal(position) => self.horizontal = position,
            ScrollCommand::Home => self.vertical = 0,
            ScrollCommand::End => self.vertical = metrics.maximum_vertical,
        }
        self.clamp(metrics);
    }

    pub fn clamp(&mut self, metrics: ViewportMetrics) {
        self.vertical = self.vertical.min(metrics.maximum_vertical);
        self.horizontal = self.horizontal.min(metrics.maximum_horizontal);
    }
}

#[must_use]
pub fn viewport_metrics(
    area: Rect,
    row_widths: &[usize],
    requested_vertical: usize,
    horizontal_enabled: bool,
) -> ViewportMetrics {
    let viewport_columns = usize::from(area.width);
    let mut horizontal = false;
    let mut columns = 0;
    for _ in 0..2 {
        let viewport_rows = usize::from(area.height).saturating_sub(usize::from(horizontal));
        let maximum_vertical = maximum_scroll(row_widths.len(), viewport_rows);
        let first = requested_vertical.min(maximum_vertical);
        columns = row_widths
            .iter()
            .skip(first)
            .take(viewport_rows)
            .copied()
            .max()
            .unwrap_or(0);
        horizontal = horizontal_enabled && columns > viewport_columns;
    }
    let horizontal_rows = u16::from(horizontal);
    let content = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(horizontal_rows),
    );
    let viewport_rows = usize::from(content.height);
    let maximum_vertical = maximum_scroll(row_widths.len(), viewport_rows);
    let first = requested_vertical.min(maximum_vertical);
    columns = row_widths
        .iter()
        .skip(first)
        .take(viewport_rows)
        .copied()
        .max()
        .unwrap_or(columns);
    ViewportMetrics {
        area: content,
        horizontal_scrollbar: if horizontal {
            Rect::new(
                area.x,
                area.bottom().saturating_sub(design::BORDER_WIDTH),
                area.width,
                design::BORDER_WIDTH,
            )
        } else {
            Rect::default()
        },
        rows: row_widths.len(),
        columns,
        viewport_rows,
        viewport_columns,
        maximum_vertical,
        maximum_horizontal: maximum_scroll(columns, viewport_columns),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_have_fixed_distances_and_bounds() {
        let metrics = ViewportMetrics {
            viewport_rows: 10,
            maximum_vertical: 30,
            maximum_horizontal: 12,
            ..ViewportMetrics::default()
        };
        let mut viewport = Viewport::default();
        viewport.apply(ScrollCommand::Lines(LINE_SCROLL_ROWS), metrics);
        assert_eq!(viewport.vertical, 4);
        viewport.apply(ScrollCommand::Page(1), metrics);
        assert_eq!(viewport.vertical, 14);
        viewport.apply(ScrollCommand::Columns(99), metrics);
        assert_eq!(viewport.horizontal, 12);
        viewport.apply(ScrollCommand::End, metrics);
        assert_eq!(viewport.vertical, 30);
        viewport.apply(ScrollCommand::Lines(-99), metrics);
        assert_eq!(viewport.vertical, 0);
    }

    #[test]
    fn horizontal_overflow_uses_only_visible_rows() {
        let widths = [200, 10, 10, 10];
        let top = viewport_metrics(Rect::new(0, 0, 20, 2), &widths, 0, true);
        let bottom = viewport_metrics(Rect::new(0, 0, 20, 2), &widths, 2, true);
        assert!(top.maximum_horizontal > 0);
        assert_eq!(bottom.maximum_horizontal, 0);
    }

    #[test]
    fn scrollbar_final_cell_maps_to_maximum() {
        assert_eq!(scrollbar_position(9, 10, 37), 37);
        assert_eq!(scrollbar_position_count(120, 25), 96);
    }

    #[test]
    fn scrollbar_targets_are_exact_and_nearby_cells_are_inert() {
        let metrics = ViewportMetrics {
            area: Rect::new(2, 3, 20, 8),
            horizontal_scrollbar: Rect::new(2, 11, 20, 1),
            maximum_vertical: 40,
            maximum_horizontal: 30,
            ..ViewportMetrics::default()
        };
        let areas = scrollbar_areas(Rect::new(2, 3, 21, 9), metrics);
        assert_eq!(
            scrollbar_axis_at(areas, metrics, 22, 10),
            Some(ScrollbarAxis::Vertical)
        );
        assert_eq!(scrollbar_axis_at(areas, metrics, 21, 10), None);
        assert_eq!(
            scrollbar_command(ScrollbarAxis::Vertical, areas, metrics, 22, 10),
            ScrollCommand::Vertical(40)
        );
        assert_eq!(
            scrollbar_command(ScrollbarAxis::Horizontal, areas, metrics, 21, 11),
            ScrollCommand::Horizontal(30)
        );
    }
}
