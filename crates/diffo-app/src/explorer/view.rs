use diffo_ui::file_picker::{Document as PickerDocument, FilePicker, TreeNode as PickerTreeNode};
use diffo_ui::text_view::render_lines;
use diffo_ui::text_view::{Viewport, ViewportMetrics, render_scrollbars, viewport_metrics};
use diffo_ui::{
    PaneSplit, change_kind_style, design, file_icons, plain_syntax_spans, terminal_safe_text,
    theme, tool_areas,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::model::{EntryId, ExplorerModel, GutterMarker, TreeEntry};

pub(crate) const VIEWER_GUTTER_WIDTH: u16 = 7;

pub(crate) struct ExplorerAreas {
    pub(crate) tree: Rect,
    pub(crate) viewer: Rect,
}

pub(crate) fn explorer_areas(area: Rect, split: PaneSplit) -> ExplorerAreas {
    let vertical = tool_areas(area);
    let horizontal = split.areas(vertical.content);
    ExplorerAreas {
        tree: horizontal.leading,
        viewer: horizontal.trailing,
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
    picker.render_menu(frame);
}

pub(crate) fn render_full_screen(
    frame: &mut Frame,
    area: Rect,
    model: &ExplorerModel,
    skeleton: bool,
) {
    let Some(viewer) = model.viewer.as_ref() else {
        return;
    };
    if let Some(message) = viewer.message.as_deref() {
        frame.render_widget(Paragraph::new(terminal_safe_text(message)), area);
        return;
    }
    let metrics = full_screen_viewer_metrics(area, model, viewer);
    let lines = if skeleton {
        Vec::new()
    } else {
        viewer
            .lines
            .iter()
            .enumerate()
            .skip(model.viewer_scroll)
            .take(metrics.viewport_rows)
            .map(|(index, line)| viewer_code(index.saturating_add(1), line, viewer))
            .collect()
    };
    render_lines(frame, metrics.area, lines, model.viewer_horizontal_scroll);
}

pub(crate) fn full_screen_viewer_metrics(
    area: Rect,
    model: &ExplorerModel,
    viewer: &super::model::Viewer,
) -> ViewportMetrics {
    let widths = viewer
        .lines
        .iter()
        .map(|line| Span::raw(terminal_safe_text(line)).width())
        .collect::<Vec<_>>();
    viewport_metrics(area, &widths, model.viewer_scroll, false)
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
    let label = Line::styled(
        terminal_safe_text(&entry_name(entry)),
        picker_entry_style(entry),
    );
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

fn picker_entry_style(entry: &TreeEntry) -> Style {
    let Some(status) = entry.status else {
        return Style::default().fg(theme::TEXT);
    };
    let status_style = change_kind_style(status);
    if entry.directory() {
        Style::default().fg(status_style.fg.unwrap_or(theme::TEXT))
    } else {
        status_style
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
    if let Some(viewer) = model.viewer.as_ref() {
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
            .bg(theme::CONFLICT_BACKGROUND),
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
    fn tree_picker_shows_non_deleted_git_colors_without_status_letters() {
        let mut model = ExplorerModel::new(&RepositorySnapshot {
            files: vec![
                FileState {
                    path: "conflicted.rs".into(),
                    old_path: None,
                    kind: ChangeKind::Conflicted,
                    staged: None,
                    unstaged: Some(FileDiff {
                        text: String::new(),
                    }),
                },
                FileState {
                    path: "modified.rs".into(),
                    old_path: None,
                    kind: ChangeKind::Modified,
                    staged: None,
                    unstaged: Some(FileDiff {
                        text: String::new(),
                    }),
                },
                FileState {
                    path: "src/added.rs".into(),
                    old_path: None,
                    kind: ChangeKind::Added,
                    staged: None,
                    unstaged: Some(FileDiff {
                        text: String::new(),
                    }),
                },
            ],
            ..RepositorySnapshot::default()
        });
        model.install_paths(vec![
            "conflicted.rs".into(),
            "modified.rs".into(),
            "src/added.rs".into(),
        ]);
        let mut picker = FilePicker::default();
        picker.prepare(
            Rect::new(0, 0, 30, 5),
            tree_document(&model, Style::default(), false),
            None,
        );
        let backend = TestBackend::new(30, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| picker.render(frame, false)).unwrap();

        insta::assert_debug_snapshot!(terminal.backend().buffer(), @r#"
        Buffer {
            area: Rect { x: 0, y: 0, width: 30, height: 5 },
            content: [
                "┌ Explorer ───────────     ┐",
                "│  conflicted.rs            │",
                "│  modified.rs              │",
                "│ src                      │",
                "└────────────────────────────┘",
            ],
            styles: [
                x: 0, y: 0, fg: Reset, bg: Reset, underline: Reset, modifier: NONE,
                x: 1, y: 0, fg: White, bg: Reset, underline: Reset, modifier: NONE,
                x: 11, y: 0, fg: Reset, bg: Reset, underline: Reset, modifier: NONE,
                x: 22, y: 0, fg: White, bg: Reset, underline: Reset, modifier: BOLD,
                x: 29, y: 0, fg: Reset, bg: Reset, underline: Reset, modifier: NONE,
                x: 1, y: 1, fg: White, bg: Reset, underline: Reset, modifier: BOLD,
                x: 3, y: 1, fg: LightRed, bg: Reset, underline: Reset, modifier: NONE,
                x: 29, y: 1, fg: Reset, bg: Reset, underline: Reset, modifier: NONE,
                x: 1, y: 2, fg: White, bg: Reset, underline: Reset, modifier: BOLD,
                x: 3, y: 2, fg: Yellow, bg: Reset, underline: Reset, modifier: NONE,
                x: 29, y: 2, fg: Reset, bg: Reset, underline: Reset, modifier: NONE,
                x: 1, y: 3, fg: White, bg: Reset, underline: Reset, modifier: BOLD,
                x: 3, y: 3, fg: LightGreen, bg: Reset, underline: Reset, modifier: NONE,
                x: 29, y: 3, fg: Reset, bg: Reset, underline: Reset, modifier: NONE,
            ]
        }
        "#);
    }

    #[test]
    fn explorer_aligns_file_and_folder_names_at_each_depth() {
        let mut model = ExplorerModel::new(&RepositorySnapshot::default());
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

        insta::assert_debug_snapshot!("collapsed", terminal.backend().buffer());

        picker.navigate(Navigation::Activate);
        terminal.draw(|frame| picker.render(frame, false)).unwrap();

        insta::assert_debug_snapshot!("expanded", terminal.backend().buffer());
    }

    #[test]
    fn viewer_title_matches_the_committed_tree_label() {
        let entry = TreeEntry {
            id: EntryId::File("deleted.rs".into()),
            status: Some(ChangeKind::Deleted),
            children: Vec::new(),
        };
        let title = entry_label(&entry);
        let mut model = ExplorerModel::new(&diffo_core::RepositorySnapshot::default());
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

        insta::assert_debug_snapshot!(terminal.backend().buffer());
    }

    #[test]
    fn horizontal_pan_keeps_the_gutter_and_renders_control_text_inertly() {
        let mut model = ExplorerModel::new(&diffo_core::RepositorySnapshot::default());
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
        assert!(!screen.chars().any(char::is_control));
        insta::assert_debug_snapshot!(terminal.backend().buffer());
    }

    #[test]
    fn rust_keywords_use_the_diff_foreground_without_background_or_modifiers() {
        let source = "fn main() {}";
        let document =
            parse_unified_patch(&format!("@@ -0,0 +1 @@\n+{source}\n")).expect("valid patch");
        let highlighted = SyntaxHighlighter::new()
            .highlight(std::path::Path::new("main.rs"), &document)
            .new;
        let keyword_foreground = highlighted[&1]
            .spans
            .first()
            .expect("highlighted Rust keyword")
            .foreground;

        let mut model = ExplorerModel::new(&diffo_core::RepositorySnapshot::default());
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

        insta::assert_debug_snapshot!(terminal.backend().buffer());
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

        let backend = TestBackend::new(40, 5);
        let mut full_screen = Terminal::new(backend).unwrap();
        full_screen
            .draw(|frame| render_full_screen(frame, frame.area(), &model, false))
            .unwrap();
        let keyword_cell = &full_screen.backend().buffer()[(0, 0)];
        assert_eq!(keyword_cell.symbol(), "f");
        assert_eq!(
            keyword_cell.fg,
            Color::Rgb(
                keyword_foreground.red,
                keyword_foreground.green,
                keyword_foreground.blue,
            )
        );
        let screen = full_screen
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(!screen.contains("   1"));
        assert!(!screen.contains('┌'));
    }

    #[test]
    fn full_screen_file_text_has_no_scroll_controls() {
        let mut model = ExplorerModel::new(&diffo_core::RepositorySnapshot::default());
        model.viewer = Some(super::super::model::Viewer {
            path: "many.txt".into(),
            title: Box::new(Line::raw("many.txt")),
            lines: (0..20).map(|line| format!("line {line}")).collect(),
            markers: HashMap::new(),
            highlighted: HashMap::new(),
            coverage: Vec::new(),
            syntax_eligible: false,
            message: None,
        });
        let backend = TestBackend::new(20, 4);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| render_full_screen(frame, frame.area(), &model, false))
            .unwrap();

        assert_eq!(terminal.backend().buffer()[(0, 0)].symbol(), "l");
        let screen = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(!screen.contains('█'));
        assert!(!screen.contains('║'));
        assert!(!screen.contains('═'));
    }

    #[test]
    fn skeleton_renders_line_numbers_without_text_or_markers() {
        let mut model = ExplorerModel::new(&diffo_core::RepositorySnapshot::default());
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

        insta::assert_debug_snapshot!(terminal.backend().buffer());
    }
}
