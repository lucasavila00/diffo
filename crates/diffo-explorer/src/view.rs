use diffo_core::ChangeKind;
use diffo_file_picker::{Document as PickerDocument, FilePicker, Row as PickerRow};
use diffo_text_view::render_lines;
use diffo_text_view::{Viewport, ViewportMetrics, render_scrollbars, viewport_metrics};
use diffo_ui::{
    PaneSplit, change_kind_style, design, plain_syntax_spans, terminal_safe_text, theme, tool_areas,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::model::{ExplorerModel, GutterMarker, TreeEntry};

pub(crate) const VIEWER_GUTTER_WIDTH: u16 = 7;

pub(crate) struct ExplorerAreas {
    pub(crate) tree: Rect,
    pub(crate) viewer: Rect,
    pub(crate) status: Rect,
}

pub(crate) fn explorer_areas(area: Rect, split: PaneSplit) -> ExplorerAreas {
    let vertical = tool_areas(area);
    let horizontal = split.areas(vertical.content);
    ExplorerAreas {
        tree: horizontal.leading,
        viewer: horizontal.trailing,
        status: vertical.status,
    }
}

pub(crate) fn render(
    frame: &mut Frame,
    area: Rect,
    split: PaneSplit,
    model: &ExplorerModel,
    picker: &FilePicker<std::path::PathBuf>,
    skeleton: bool,
) {
    frame.render_widget(Clear, area);
    let areas = explorer_areas(area, split);
    let border_style = split.border_style();
    picker.render(frame, true);
    render_viewer(frame, areas.viewer, model, border_style, skeleton);
    frame.render_widget(
        Paragraph::new(
            " j/w: previous  k/l/s: next  enter/click: expand  1/f1: commands  ↑/↓: scroll ",
        ),
        areas.status,
    );
    picker.render_menu(frame);
}

pub(crate) fn tree_document(
    model: &ExplorerModel,
    border_style: Style,
    loading: bool,
) -> PickerDocument<std::path::PathBuf> {
    let rows = model
        .entries
        .iter()
        .map(|entry| {
            let name = entry
                .path
                .file_name()
                .unwrap_or(entry.path.as_os_str())
                .to_string_lossy();
            let prefix = if entry.directory {
                ""
            } else {
                match entry.status {
                    Some(ChangeKind::Added | ChangeKind::Untracked) => "A ",
                    Some(ChangeKind::Modified) => "M ",
                    Some(ChangeKind::Deleted) => "D ",
                    Some(ChangeKind::Renamed) => "R ",
                    Some(ChangeKind::Copied) => "C ",
                    Some(ChangeKind::Conflicted) => "U ",
                    None => "  ",
                }
            };
            PickerRow::tree(
                entry.path.clone(),
                Line::styled(
                    terminal_safe_text(&format!("{prefix}{name}")),
                    entry_style(entry),
                ),
                entry.depth,
                entry.directory,
            )
        })
        .collect();
    let mut document = PickerDocument::tree("Explorer", rows);
    document.border_style = border_style;
    if loading {
        "Loading files…".clone_into(&mut document.empty_message);
    }
    document
}

fn entry_style(entry: &TreeEntry) -> Style {
    entry.status.map_or_else(
        || {
            Style::default().fg(if entry.directory {
                theme::CHROME
            } else {
                theme::TEXT
            })
        },
        status_style,
    )
}

fn status_style(kind: ChangeKind) -> Style {
    change_kind_style(kind, false)
}

fn render_viewer(
    frame: &mut Frame,
    area: Rect,
    model: &ExplorerModel,
    border_style: Style,
    skeleton: bool,
) {
    let title = model.viewer.as_ref().map_or_else(
        || " File Viewer ".to_owned(),
        |viewer| terminal_safe_text(&format!(" {} ", viewer.path.display())),
    );
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title),
        area,
    );
    let inner = area.inner(design::PANEL_INSET);
    if let Some(error) = model.error.as_deref() {
        frame.render_widget(
            Paragraph::new(terminal_safe_text(error)).style(Style::default().fg(theme::DANGER)),
            inner,
        );
    } else if let Some(viewer) = model.viewer.as_ref() {
        if let Some(message) = viewer.message.as_deref() {
            frame.render_widget(Paragraph::new(terminal_safe_text(message)), inner);
        } else {
            let metrics = viewer_metrics(inner, model, viewer);
            render_viewer_lines(frame, metrics.area, model, viewer, skeleton);
            render_scrollbars(
                frame,
                inner,
                metrics,
                Viewport {
                    vertical: model.viewer_scroll,
                    horizontal: model.viewer_horizontal_scroll,
                },
            );
        }
    } else {
        frame.render_widget(Paragraph::new("Select a file to view it."), inner);
    }
}

pub(crate) fn viewer_metrics(
    area: Rect,
    model: &ExplorerModel,
    viewer: &super::model::Viewer,
) -> ViewportMetrics {
    let text_area = Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(design::BORDER_WIDTH),
        area.height,
    );
    let widths = viewer
        .lines
        .iter()
        .map(|line| {
            Span::raw(terminal_safe_text(line))
                .width()
                .saturating_add(usize::from(VIEWER_GUTTER_WIDTH))
        })
        .collect::<Vec<_>>();
    viewport_metrics(text_area, &widths, model.viewer_scroll, true)
}

fn render_viewer_lines(
    frame: &mut Frame,
    area: Rect,
    model: &ExplorerModel,
    viewer: &super::model::Viewer,
    skeleton: bool,
) {
    let columns = Layout::horizontal([
        Constraint::Length(VIEWER_GUTTER_WIDTH.min(area.width)),
        Constraint::Min(0),
    ])
    .split(area);
    let visible = viewer
        .lines
        .iter()
        .enumerate()
        .skip(model.viewer_scroll)
        .take(usize::from(area.height))
        .collect::<Vec<_>>();
    let gutters = visible
        .iter()
        .map(|(index, _)| viewer_gutter(index.saturating_add(1), viewer, skeleton))
        .collect::<Vec<_>>();
    let code = if skeleton {
        Vec::new()
    } else {
        visible
            .into_iter()
            .map(|(index, text)| viewer_code(index.saturating_add(1), text, viewer))
            .collect::<Vec<_>>()
    };
    frame.render_widget(Paragraph::new(gutters), columns[0]);
    render_lines(frame, columns[1], code, model.viewer_horizontal_scroll);
}

fn viewer_gutter(number: usize, viewer: &super::model::Viewer, skeleton: bool) -> Line<'static> {
    let marker = (!skeleton)
        .then(|| viewer.markers.get(&number).copied())
        .flatten();
    let marker_text = if marker.is_some() { "▌" } else { " " };
    Line::from(vec![
        Span::styled(format!("{number:>4} "), Style::default().fg(theme::CHROME)),
        Span::styled(marker_text, marker_style(marker)),
        Span::raw(" "),
    ])
}

fn viewer_code(number: usize, text: &str, viewer: &super::model::Viewer) -> Line<'static> {
    let number = u32::try_from(number).unwrap_or(u32::MAX);
    if let Some(highlighted) = viewer.highlighted.get(&number) {
        Line::from(plain_syntax_spans(highlighted))
    } else {
        Line::raw(terminal_safe_text(text))
    }
}

fn marker_style(marker: Option<GutterMarker>) -> Style {
    match marker {
        Some(GutterMarker::Added) => Style::default().fg(theme::SUCCESS),
        Some(GutterMarker::Modified) => Style::default().fg(theme::WARNING),
        Some(GutterMarker::Deleted) => Style::default().fg(theme::DANGER),
        Some(GutterMarker::Conflict) => Style::default()
            .fg(theme::CONFLICT_FOREGROUND)
            .bg(theme::CONFLICT_BACKGROUND)
            .add_modifier(Modifier::BOLD),
        None => Style::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diffo_core::{FileDiff, FileState, RepositorySnapshot};
    use ratatui::{Terminal, backend::TestBackend, style::Color};
    use std::collections::HashMap;

    #[test]
    fn tree_statuses_reuse_diff_colors_and_unchanged_entries_are_neutral() {
        assert_eq!(status_style(ChangeKind::Added).fg, Some(Color::LightGreen));
        assert_eq!(status_style(ChangeKind::Modified).fg, Some(Color::Yellow));
        assert_eq!(status_style(ChangeKind::Deleted).fg, Some(Color::LightRed));

        let unchanged = TreeEntry {
            path: "plain.txt".into(),
            depth: 0,
            directory: false,
            status: None,
        };
        assert_eq!(entry_style(&unchanged).fg, Some(Color::White));

        let directory = TreeEntry {
            path: "src".into(),
            depth: 0,
            directory: true,
            status: None,
        };
        assert_eq!(entry_style(&directory).fg, Some(theme::CHROME));
    }

    #[test]
    fn tree_picker_renders_the_file_status_color() {
        let mut model = ExplorerModel::new(RepositorySnapshot {
            files: vec![FileState {
                path: "changed.rs".into(),
                old_path: None,
                kind: ChangeKind::Modified,
                staged: None,
                unstaged: Some(FileDiff {
                    text: String::new(),
                }),
            }],
            ..RepositorySnapshot::default()
        });
        model.install_paths(Vec::new());
        let mut picker = FilePicker::default();
        picker.prepare(
            Rect::new(0, 0, 30, 4),
            tree_document(&model, Style::default(), false),
            None,
        );
        let backend = TestBackend::new(30, 4);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| picker.render(frame, false)).unwrap();

        let marker = &terminal.backend().buffer()[(5, 1)];
        assert_eq!(marker.symbol(), "M");
        assert_eq!(marker.fg, Color::Yellow);
    }

    #[test]
    fn horizontal_pan_keeps_the_gutter_and_renders_control_text_inertly() {
        let mut model = ExplorerModel::new(diffo_core::RepositorySnapshot::default());
        model.viewer = Some(super::super::model::Viewer {
            path: "wide.txt".into(),
            lines: vec!["01234567\x1b[2JPAN_TARGET".to_owned()],
            markers: HashMap::new(),
            highlighted: HashMap::new(),
            coverage: Vec::new(),
            syntax_eligible: false,
            message: None,
        });
        model.viewer_horizontal_scroll = 8;
        let backend = TestBackend::new(30, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| render_viewer(frame, frame.area(), &model, Style::default(), false))
            .unwrap();

        let screen = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(screen.contains("   1"));
        assert!(screen.contains("␛[2JPAN_TARGET"));
        assert!(!screen.chars().any(char::is_control));
    }

    #[test]
    fn skeleton_renders_line_numbers_without_text_or_markers() {
        let mut model = ExplorerModel::new(diffo_core::RepositorySnapshot::default());
        model.viewer = Some(super::super::model::Viewer {
            path: "pending.rs".into(),
            lines: vec!["TEXT_MUST_BE_HIDDEN".to_owned()],
            markers: HashMap::from([(1, GutterMarker::Added)]),
            highlighted: HashMap::new(),
            coverage: Vec::new(),
            syntax_eligible: true,
            message: None,
        });
        let backend = TestBackend::new(40, 4);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| render_viewer(frame, frame.area(), &model, Style::default(), true))
            .unwrap();

        let screen = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(screen.contains("   1"));
        assert!(!screen.contains("TEXT_MUST_BE_HIDDEN"));
        assert!(!screen.contains('▌'));
    }
}
