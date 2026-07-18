use diffo_core::ChangeKind;
use diffo_ui::{change_kind_style, plain_syntax_spans, terminal_safe_text, tool_areas};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph},
};

use super::model::{ExplorerModel, GutterMarker, TreeEntry};

pub(crate) const VIEWER_GUTTER_WIDTH: u16 = 7;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TreeAction {
    CollapseAll,
    ExpandAll,
}

pub(crate) struct ExplorerAreas {
    pub(crate) tree: Rect,
    pub(crate) viewer: Rect,
    pub(crate) status: Rect,
}

pub(crate) fn explorer_areas(area: Rect) -> ExplorerAreas {
    let vertical = tool_areas(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(vertical.content);
    ExplorerAreas {
        tree: horizontal[0],
        viewer: horizontal[1],
        status: vertical.status,
    }
}

pub(crate) fn tree_action_at(area: Rect, column: u16, row: u16) -> Option<TreeAction> {
    if row != area.y || area.width < 12 {
        return None;
    }
    let start = area.right().saturating_sub(8);
    if column >= start && column < start.saturating_add(3) {
        Some(TreeAction::CollapseAll)
    } else if column >= start.saturating_add(4) && column < start.saturating_add(7) {
        Some(TreeAction::ExpandAll)
    } else {
        None
    }
}

pub(crate) fn render(frame: &mut Frame, area: Rect, model: &ExplorerModel) {
    frame.render_widget(Clear, area);
    let areas = explorer_areas(area);
    render_tree(frame, areas.tree, model);
    render_viewer(frame, areas.viewer, model);
    frame.render_widget(
        Paragraph::new(
            " j/k: select  enter/click: expand  [-]/[+]: fold all  1/f1: commands  ↑/↓: scroll ",
        ),
        areas.status,
    );
}

fn render_tree(frame: &mut Frame, area: Rect, model: &ExplorerModel) {
    let inner = area.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    let items = model
        .visible
        .iter()
        .skip(model.tree_scroll)
        .take(usize::from(inner.height))
        .enumerate()
        .map(|(offset, entry)| {
            tree_item(
                entry,
                model.tree_scroll.saturating_add(offset) == model.selected,
            )
        });
    let selected = model
        .selected
        .checked_sub(model.tree_scroll)
        .filter(|index| *index < usize::from(inner.height));
    let mut block = Block::default().borders(Borders::ALL).title(" Explorer ");
    if area.width >= 12 {
        block = block.title(Line::from("[-] [+]").alignment(Alignment::Right));
    }
    frame.render_widget(block, area);
    let list = List::new(items)
        .highlight_style(Style::default())
        .highlight_symbol("› ")
        .highlight_spacing(HighlightSpacing::Always);
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, inner, &mut state);
}

fn tree_item(entry: &TreeEntry, selected: bool) -> ListItem<'static> {
    let name = entry
        .path
        .file_name()
        .unwrap_or(entry.path.as_os_str())
        .to_string_lossy();
    let name = terminal_safe_text(&name);
    let prefix = if entry.directory {
        if entry.expanded { "▾ " } else { "▸ " }
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
    let style = entry_style(entry, selected);
    ListItem::new(Line::styled(
        format!("{}{}{}", "  ".repeat(entry.depth), prefix, name),
        style,
    ))
}

fn entry_style(entry: &TreeEntry, selected: bool) -> Style {
    let style = entry.status.map_or_else(
        || {
            Style::default().fg(if entry.directory {
                Color::Gray
            } else {
                Color::White
            })
        },
        status_style,
    );
    if selected && !entry.directory {
        style.bg(Color::DarkGray).add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

fn status_style(kind: ChangeKind) -> Style {
    change_kind_style(kind, false)
}

fn render_viewer(frame: &mut Frame, area: Rect, model: &ExplorerModel) {
    let title = model.viewer.as_ref().map_or_else(
        || " File Viewer ".to_owned(),
        |viewer| terminal_safe_text(&format!(" {} ", viewer.path.display())),
    );
    frame.render_widget(Block::default().borders(Borders::ALL).title(title), area);
    let inner = area.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    if let Some(error) = model.error.as_deref() {
        frame.render_widget(
            Paragraph::new(terminal_safe_text(error)).style(Style::default().fg(Color::Red)),
            inner,
        );
    } else if let Some(viewer) = model.viewer.as_ref() {
        if let Some(message) = viewer.message.as_deref() {
            frame.render_widget(Paragraph::new(terminal_safe_text(message)), inner);
        } else {
            render_viewer_lines(frame, inner, model, viewer);
        }
    } else {
        frame.render_widget(Paragraph::new("Select a file to view it."), inner);
    }
}

fn render_viewer_lines(
    frame: &mut Frame,
    area: Rect,
    model: &ExplorerModel,
    viewer: &super::model::Viewer,
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
        .map(|(index, _)| viewer_gutter(index.saturating_add(1), viewer))
        .collect::<Vec<_>>();
    let code = visible
        .into_iter()
        .map(|(index, text)| viewer_code(index.saturating_add(1), text, viewer))
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(gutters), columns[0]);
    frame.render_widget(
        Paragraph::new(code).scroll((
            0,
            model
                .viewer_horizontal_scroll
                .try_into()
                .unwrap_or(u16::MAX),
        )),
        columns[1],
    );
}

fn viewer_gutter(number: usize, viewer: &super::model::Viewer) -> Line<'static> {
    let marker = viewer.markers.get(&number).copied();
    let marker_text = if marker.is_some() { "▌" } else { " " };
    Line::from(vec![
        Span::styled(
            format!("{number:>4} "),
            Style::default().fg(Color::DarkGray),
        ),
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
        Some(GutterMarker::Added) => Style::default().fg(Color::LightGreen),
        Some(GutterMarker::Modified) => Style::default().fg(Color::Yellow),
        Some(GutterMarker::Deleted) => Style::default().fg(Color::LightRed),
        Some(GutterMarker::Conflict) => Style::default()
            .fg(Color::LightYellow)
            .bg(Color::Indexed(58))
            .add_modifier(Modifier::BOLD),
        None => Style::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
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
            expanded: false,
            status: None,
        };
        assert_eq!(entry_style(&unchanged, false).fg, Some(Color::White));
        assert_eq!(entry_style(&unchanged, true).bg, Some(Color::DarkGray));

        let directory = TreeEntry {
            path: "src".into(),
            depth: 0,
            directory: true,
            expanded: false,
            status: None,
        };
        assert_eq!(entry_style(&directory, true).bg, None);
    }

    #[test]
    fn tree_header_actions_have_separate_click_targets() {
        let area = Rect::new(5, 7, 30, 20);
        assert_eq!(tree_action_at(area, 27, 7), Some(TreeAction::CollapseAll));
        assert_eq!(tree_action_at(area, 31, 7), Some(TreeAction::ExpandAll));
        assert_eq!(tree_action_at(area, 30, 7), None);
        assert_eq!(tree_action_at(area, 31, 8), None);
    }

    #[test]
    fn horizontal_pan_keeps_the_gutter_and_renders_control_text_inertly() {
        let mut model = ExplorerModel::new(diffo_core::RepositorySnapshot::default());
        model.viewer = Some(super::super::model::Viewer {
            path: "wide.txt".into(),
            lines: vec!["01234567\x1b[2JPAN_TARGET".to_owned()],
            markers: HashMap::new(),
            highlighted: HashMap::new(),
            coverage: None,
            syntax_eligible: false,
            message: None,
            maximum_width: 21,
        });
        model.viewer_horizontal_scroll = 8;
        let backend = TestBackend::new(30, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| render_viewer(frame, frame.area(), &model))
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
}
