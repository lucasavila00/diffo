use diffo_core::Commit;
use diffo_ui::{
    PaneSplit, design,
    file_picker::{Document, FilePicker, Row},
    terminal_safe_text,
    text_view::{Viewport, ViewportMetrics, render_lines, render_scrollbars, viewport_metrics},
    theme, tool_areas,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::prepare::PreparedPatch;

pub(super) struct HistoryAreas {
    pub(super) commits: Rect,
    pub(super) patch: Rect,
}

pub(super) fn areas(area: Rect, split: PaneSplit) -> HistoryAreas {
    let content = tool_areas(area).content;
    let panes = split.areas(content);
    HistoryAreas {
        commits: panes.leading,
        patch: panes.trailing,
    }
}

pub(super) fn commit_document(
    commits: &[Commit],
    border_style: Style,
    loading: bool,
) -> Document<String> {
    let rows = commits
        .iter()
        .map(|commit| Row {
            id: commit.id.clone(),
            label: Line::from(vec![
                Span::styled(
                    format!("{} ", commit.id.get(..7).unwrap_or(&commit.id)),
                    Style::default().fg(theme::CHROME),
                ),
                Span::styled(
                    terminal_safe_text(&commit.summary),
                    Style::default().fg(theme::TEXT),
                ),
            ]),
            action: None,
            context_menu: false,
            destructive_action: None,
        })
        .collect();
    let mut document = Document::flat("History", rows);
    document.border_style = border_style;
    document.empty_message = if loading {
        "Loading commits…".to_owned()
    } else {
        "No commits on this checkout.".to_owned()
    };
    document
}

pub(super) fn patch_metrics(
    area: Rect,
    patch: Option<&PreparedPatch>,
    scroll: usize,
) -> ViewportMetrics {
    let inner = area.inner(design::PANEL_INSET);
    let text = Rect::new(
        inner.x,
        inner.y,
        inner.width.saturating_sub(design::BORDER_WIDTH),
        inner.height,
    );
    patch.map_or_else(
        || viewport_metrics(text, &[], scroll, true),
        |patch| viewport_metrics(text, &patch.widths, scroll, true),
    )
}

pub(super) fn render(
    frame: &mut Frame,
    area: Rect,
    split: PaneSplit,
    picker: &FilePicker<String>,
    patch: Option<&PreparedPatch>,
    scroll: usize,
    horizontal: usize,
) {
    frame.render_widget(Clear, area);
    let areas = areas(area, split);
    picker.render(frame, patch.is_some());
    let title = patch.map_or_else(|| Line::raw(" Commit Diff "), PreparedPatch::title);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(split.border_style())
            .title(title),
        areas.patch,
    );
    let inner = areas.patch.inner(design::PANEL_INSET);
    let Some(patch) = patch else {
        frame.render_widget(Paragraph::new("Select a commit to review it."), inner);
        return;
    };
    if patch.lines.is_empty() {
        frame.render_widget(Paragraph::new("Commit contains no file changes."), inner);
        return;
    }
    let metrics = patch_metrics(areas.patch, Some(patch), scroll);
    let lines = patch
        .lines
        .iter()
        .skip(scroll)
        .take(metrics.viewport_rows)
        .cloned()
        .collect();
    render_lines(frame, metrics.area, lines, horizontal);
    render_scrollbars(
        frame,
        inner,
        metrics,
        Viewport {
            vertical: scroll,
            horizontal,
        },
    );
}

pub(super) fn render_full_screen(
    frame: &mut Frame,
    area: Rect,
    patch: &PreparedPatch,
    scroll: usize,
    horizontal: usize,
) {
    let metrics = viewport_metrics(area, &patch.widths, scroll, true);
    let lines = patch
        .lines
        .iter()
        .skip(scroll)
        .take(metrics.viewport_rows)
        .cloned()
        .collect();
    render_lines(frame, metrics.area, lines, horizontal);
}
