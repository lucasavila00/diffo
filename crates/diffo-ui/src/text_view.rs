//! Shared read-only text viewport, scrolling, and rendering.

use diffo_highlight::LineRange;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Paragraph, ScrollbarOrientation},
};
use std::{collections::BTreeMap, ops::Range};

use crate::{design, maximum_scroll, render_scrollbar, scroll_offset, scrollbar_position, theme};

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
const MAX_SYNTAX_COVERAGE_WINDOWS: usize = 8;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PreparedVerticalScroll {
    requested: Option<usize>,
}

#[must_use]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SyntaxCoverage {
    windows: Vec<LineRange>,
}

impl SyntaxCoverage {
    pub fn from_range(range: Option<LineRange>) -> Self {
        Self {
            windows: range.into_iter().collect(),
        }
    }

    #[must_use]
    pub fn covers(&self, needed: Option<LineRange>) -> bool {
        needed.is_none_or(|needed| {
            self.windows
                .iter()
                .any(|coverage| coverage.start <= needed.start && coverage.end >= needed.end)
        })
    }

    pub fn merge(&mut self, incoming: impl IntoIterator<Item = LineRange>) {
        for mut range in incoming {
            let mut insertion = self.windows.len();
            while let Some(position) = self.windows.iter().position(|existing| {
                existing.start <= range.end.saturating_add(1)
                    && range.start <= existing.end.saturating_add(1)
            }) {
                let existing = self.windows.remove(position);
                insertion = insertion.min(position);
                range.start = range.start.min(existing.start);
                range.end = range.end.max(existing.end);
            }
            self.windows
                .insert(insertion.min(self.windows.len()), range);
        }
        if self.windows.len() > MAX_SYNTAX_COVERAGE_WINDOWS {
            self.windows
                .drain(..self.windows.len() - MAX_SYNTAX_COVERAGE_WINDOWS);
        }
    }

    pub fn retain_styles<T>(&self, styles: &mut BTreeMap<u32, T>) {
        styles.retain(|line, _| self.windows.iter().any(|range| range.contains(*line)));
    }

    #[must_use]
    pub fn last(&self) -> Option<&LineRange> {
        self.windows.last()
    }

    pub fn iter(&self) -> impl Iterator<Item = &LineRange> {
        self.windows.iter()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }
}

impl FromIterator<LineRange> for SyntaxCoverage {
    fn from_iter<T: IntoIterator<Item = LineRange>>(iter: T) -> Self {
        let mut coverage = Self::default();
        coverage.merge(iter);
        coverage
    }
}

impl From<Vec<LineRange>> for SyntaxCoverage {
    fn from(windows: Vec<LineRange>) -> Self {
        windows.into_iter().collect()
    }
}

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
        render_scrollbar(
            frame,
            areas.vertical,
            &ScrollbarOrientation::VerticalRight,
            metrics.rows,
            metrics.viewport_rows,
            viewport.vertical,
            style,
        );
    }
    if metrics.maximum_horizontal > 0 {
        render_scrollbar(
            frame,
            areas.horizontal,
            &ScrollbarOrientation::HorizontalBottom,
            metrics.columns,
            metrics.viewport_columns,
            viewport.horizontal,
            style,
        );
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

impl PreparedVerticalScroll {
    pub fn request(
        &mut self,
        command: ScrollCommand,
        committed: usize,
        metrics: ViewportMetrics,
    ) -> Option<usize> {
        if matches!(
            command,
            ScrollCommand::Columns(_) | ScrollCommand::Horizontal(_)
        ) {
            return None;
        }
        let mut viewport = Viewport {
            vertical: self.requested.unwrap_or(committed),
            horizontal: 0,
        };
        viewport.apply(command, metrics);
        self.requested = Some(viewport.vertical);
        self.requested
    }

    #[must_use]
    pub const fn requested(self) -> Option<usize> {
        self.requested
    }

    pub fn take_ready(&mut self, ready: bool) -> Option<usize> {
        if ready { self.requested.take() } else { None }
    }

    pub fn clear(&mut self) {
        self.requested = None;
    }
}

#[must_use]
pub fn centered_window(
    target: usize,
    total_rows: usize,
    viewport_rows: usize,
    window_viewports: usize,
) -> Range<usize> {
    let viewport_rows = viewport_rows.max(1);
    let window_viewports = window_viewports.max(1);
    let window_rows = viewport_rows.saturating_mul(window_viewports);
    let rows_before = viewport_rows.saturating_mul(window_viewports / 2);
    let start = target
        .saturating_sub(rows_before)
        .min(total_rows.saturating_sub(window_rows));
    start..start.saturating_add(window_rows).min(total_rows)
}

#[must_use]
pub fn syntax_prefetch_viewports(
    committed: usize,
    requested: usize,
    viewport_rows: usize,
) -> usize {
    match requested.abs_diff(committed) {
        distance if distance >= viewport_rows.max(1) => 13,
        1.. => 7,
        0 => 3,
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
    fn prepared_scroll_accumulates_reverses_and_waits_for_readiness() {
        let metrics = ViewportMetrics {
            viewport_rows: 10,
            maximum_vertical: 100,
            ..ViewportMetrics::default()
        };
        let mut scroll = PreparedVerticalScroll::default();

        assert_eq!(
            scroll.request(ScrollCommand::Page(-1), 50, metrics),
            Some(40)
        );
        assert_eq!(
            scroll.request(ScrollCommand::Page(-1), 50, metrics),
            Some(30)
        );
        assert_eq!(
            scroll.request(ScrollCommand::Lines(4), 50, metrics),
            Some(34)
        );
        assert_eq!(scroll.take_ready(false), None);
        assert_eq!(scroll.requested(), Some(34));
        assert_eq!(scroll.take_ready(true), Some(34));
        assert_eq!(scroll.requested(), None);
    }

    #[test]
    fn centered_syntax_windows_and_prefetch_sizes_ignore_direction() {
        assert_eq!(centered_window(50, 100, 10, 3), 40..70);
        assert_eq!(centered_window(0, 100, 10, 3), 0..30);
        assert_eq!(centered_window(95, 100, 10, 3), 70..100);
        assert_eq!(syntax_prefetch_viewports(50, 46, 10), 7);
        assert_eq!(syntax_prefetch_viewports(50, 54, 10), 7);
        assert_eq!(syntax_prefetch_viewports(50, 40, 10), 13);
        assert_eq!(syntax_prefetch_viewports(50, 60, 10), 13);
    }

    #[test]
    fn syntax_coverage_merges_bounds_and_evicts_styles_for_every_text_surface() {
        let mut coverage = SyntaxCoverage::from_range(Some(LineRange::new(10, 20)));
        coverage.merge([LineRange::new(30, 40), LineRange::new(21, 29)]);
        assert!(coverage.covers(Some(LineRange::new(10, 40))));

        let mut styles = BTreeMap::from([(10, "first"), (40, "last"), (50, "outside")]);
        coverage.retain_styles(&mut styles);
        assert_eq!(styles.into_keys().collect::<Vec<_>>(), [10, 40]);

        let mut bounded = SyntaxCoverage::default();
        bounded.merge((0..9).map(|index| {
            let line = index * 10 + 1;
            LineRange::new(line, line)
        }));
        assert!(!bounded.covers(Some(LineRange::new(1, 1))));
        assert!(bounded.covers(Some(LineRange::new(81, 81))));
        assert_eq!(bounded.windows.len(), MAX_SYNTAX_COVERAGE_WINDOWS);
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
        assert_eq!(crate::scrollbar_position_count(120, 25), 96);
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
