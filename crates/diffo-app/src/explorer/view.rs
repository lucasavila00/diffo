use diffo_ui::file_picker::{Document as PickerDocument, FilePicker, TreeNode as PickerTreeNode};
use diffo_ui::text_view::render_lines;
use diffo_ui::text_view::{Viewport, ViewportMetrics, render_scrollbars, viewport_metrics};
use diffo_ui::{
    PaneSplit, design, file_icons, plain_syntax_spans, terminal_safe_text, theme, tool_areas,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::model::{EntryId, ExplorerModel, GutterMarker, TreeEntry};

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
    picker: &FilePicker<EntryId>,
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
) -> PickerDocument<EntryId> {
    let rows = model.entries.iter().map(picker_tree_node).collect();
    let mut document = PickerDocument::tree("Explorer", rows);
    document.border_style = border_style;
    if loading {
        "Loading files…".clone_into(&mut document.empty_message);
    }
    document
}

fn picker_tree_node(entry: &TreeEntry) -> PickerTreeNode<EntryId> {
    let label = entry_label(entry);
    if entry.directory() {
        PickerTreeNode::branch(
            entry.id.clone(),
            label,
            entry.children.iter().map(picker_tree_node).collect(),
        )
    } else {
        PickerTreeNode::leaf(entry.id.clone(), label)
    }
}

pub(crate) fn entry_label(entry: &TreeEntry) -> Line<'static> {
    Line::styled(
        terminal_safe_text(&entry_name(entry)),
        Style::default().fg(theme::TEXT),
    )
}

fn entry_name(entry: &TreeEntry) -> String {
    let name = entry
        .path()
        .file_name()
        .unwrap_or(entry.path().as_os_str())
        .to_string_lossy();
    if entry.directory() {
        name.into_owned()
    } else {
        format!("{}{name}", file_icons::file_icon(entry.path()))
    }
}

fn render_viewer(
    frame: &mut Frame,
    area: Rect,
    model: &ExplorerModel,
    border_style: Style,
    skeleton: bool,
) {
    let title = model.viewer.as_ref().map_or_else(
        || Line::raw(" File Viewer "),
        |viewer| viewer.title.as_ref().clone(),
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
    use diffo_core::{ChangeKind, FileDiff, FileState, RepositorySnapshot};
    use diffo_diff::parse_unified_patch;
    use diffo_highlight::SyntaxHighlighter;
    use diffo_ui::file_picker::Navigation;
    use ratatui::{Terminal, backend::TestBackend, style::Color};
    use std::collections::HashMap;

    #[test]
    fn tree_picker_omits_the_file_status_letter_and_color() {
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

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(1, 1)].symbol(), " ");
        assert_eq!(buffer[(2, 1)].symbol(), " ");
        let icon = &buffer[(3, 1)];
        assert_eq!(
            icon.symbol(),
            file_icons::file_icon(std::path::Path::new("changed.rs"))
        );
        assert_eq!(icon.fg, theme::TEXT);
        assert_eq!(buffer[(4, 1)].symbol(), "c");
    }

    #[test]
    fn explorer_aligns_file_and_folder_names_at_each_depth() {
        let mut model = ExplorerModel::new(RepositorySnapshot::default());
        model.install_paths(vec![
            "directory/nested/child.rs".into(),
            "directory/plain.rs".into(),
            "file.txt".into(),
        ]);
        let mut picker = FilePicker::default();
        picker.prepare(
            Rect::new(0, 0, 30, 6),
            tree_document(&model, Style::default(), false),
            None,
        );
        let backend = TestBackend::new(30, 6);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| picker.render(frame, false)).unwrap();

        let column_of = |terminal: &Terminal<TestBackend>, row, symbol| {
            (0..30)
                .find(|column| terminal.backend().buffer()[(*column, row)].symbol() == symbol)
                .unwrap()
        };
        assert_eq!(column_of(&terminal, 1, "d"), column_of(&terminal, 2, "f"));

        picker.navigate(Navigation::Activate);
        terminal.draw(|frame| picker.render(frame, false)).unwrap();

        assert_eq!(column_of(&terminal, 2, "n"), column_of(&terminal, 3, "p"));
    }

    #[test]
    fn viewer_title_matches_the_committed_tree_label() {
        let entry = TreeEntry {
            id: EntryId::File("deleted.rs".into()),
            status: Some(ChangeKind::Deleted),
            children: Vec::new(),
        };
        let title = entry_label(&entry);
        let mut model = ExplorerModel::new(diffo_core::RepositorySnapshot::default());
        model.viewer = Some(super::super::model::Viewer {
            path: "different-path.rs".into(),
            title: Box::new(title),
            lines: Vec::new(),
            markers: HashMap::new(),
            highlighted: HashMap::new(),
            coverage: Vec::new(),
            syntax_eligible: false,
            message: None,
        });
        let backend = TestBackend::new(30, 4);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| render_viewer(frame, frame.area(), &model, Style::default(), false))
            .unwrap();

        let expected = "deleted.rs";
        for (offset, expected) in expected.chars().enumerate() {
            let cell = &terminal.backend().buffer()[(u16::try_from(offset).unwrap() + 1, 0)];
            assert_eq!(cell.symbol(), expected.to_string());
            assert_eq!(cell.fg, theme::TEXT);
            assert!(!cell.modifier.contains(Modifier::CROSSED_OUT));
        }
        let screen = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(!screen.contains("different-path.rs"));
    }

    #[test]
    fn horizontal_pan_keeps_the_gutter_and_renders_control_text_inertly() {
        let mut model = ExplorerModel::new(diffo_core::RepositorySnapshot::default());
        model.viewer = Some(super::super::model::Viewer {
            path: "wide.txt".into(),
            title: Box::new(Line::raw("  wide.txt")),
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
    fn rust_keywords_use_the_diff_foreground_without_background_or_modifiers() {
        let source = "fn main() {}";
        let document =
            parse_unified_patch(&format!("@@ -0,0 +1 @@\n+{source}\n")).expect("valid patch");
        let highlighted = SyntaxHighlighter::new()
            .highlight(std::path::Path::new("main.rs"), &document)
            .new;
        let keyword = highlighted[&1]
            .spans
            .first()
            .expect("highlighted Rust keyword");
        assert_eq!(keyword.text, "fn");
        let keyword_foreground = keyword.foreground;

        let mut model = ExplorerModel::new(diffo_core::RepositorySnapshot::default());
        model.viewer = Some(super::super::model::Viewer {
            path: "main.rs".into(),
            title: Box::new(Line::raw("  main.rs")),
            lines: vec![source.to_owned()],
            markers: HashMap::new(),
            highlighted: highlighted.into_iter().collect(),
            coverage: vec![diffo_highlight::LineRange::new(1, 1)],
            syntax_eligible: true,
            message: None,
        });
        let backend = TestBackend::new(40, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| render_viewer(frame, frame.area(), &model, Style::default(), false))
            .unwrap();

        let code_area = terminal.backend().buffer().area.inner(design::PANEL_INSET);
        let keyword_cell =
            &terminal.backend().buffer()[(code_area.x + VIEWER_GUTTER_WIDTH, code_area.y)];
        assert_eq!(
            keyword_cell.fg,
            Color::Rgb(
                keyword_foreground.red,
                keyword_foreground.green,
                keyword_foreground.blue,
            )
        );
        assert_eq!(keyword_cell.bg, Color::Reset);
        assert!(keyword_cell.modifier.is_empty());
    }

    #[test]
    fn skeleton_renders_line_numbers_without_text_or_markers() {
        let mut model = ExplorerModel::new(diffo_core::RepositorySnapshot::default());
        model.viewer = Some(super::super::model::Viewer {
            path: "pending.rs".into(),
            title: Box::new(Line::raw("  pending.rs")),
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
