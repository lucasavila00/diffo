use diffo_core::ChangeKind;
use diffo_tui::{change_kind_style, plain_syntax_spans};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph},
};

use super::model::{ExplorerModel, GutterMarker, TreeEntry};

pub(crate) struct ExplorerAreas {
    pub(crate) tree: Rect,
    pub(crate) viewer: Rect,
    pub(crate) status: Rect,
}

pub(crate) fn explorer_areas(area: Rect) -> ExplorerAreas {
    let vertical = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(vertical[0]);
    ExplorerAreas {
        tree: horizontal[0],
        viewer: horizontal[1],
        status: vertical[1],
    }
}

pub(crate) fn render(frame: &mut Frame, area: Rect, model: &ExplorerModel) {
    frame.render_widget(Clear, area);
    let areas = explorer_areas(area);
    render_tree(frame, areas.tree, model);
    render_viewer(frame, areas.viewer, model);
    frame.render_widget(
        Paragraph::new(" j/k: select  enter: expand  ↑/↓: scroll  ←/→: pan "),
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
    frame.render_widget(
        Block::default().borders(Borders::ALL).title(" Explorer "),
        area,
    );
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
        |viewer| format!(" {} ", viewer.path.display()),
    );
    frame.render_widget(Block::default().borders(Borders::ALL).title(title), area);
    let inner = area.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    let lines = if let Some(error) = model.error.as_deref() {
        vec![Line::styled(
            error.to_owned(),
            Style::default().fg(Color::Red),
        )]
    } else if let Some(viewer) = model.viewer.as_ref() {
        if let Some(message) = viewer.message.as_deref() {
            vec![Line::raw(message.to_owned())]
        } else {
            viewer
                .lines
                .iter()
                .enumerate()
                .skip(model.viewer_scroll)
                .take(usize::from(inner.height))
                .map(|(index, text)| viewer_line(index.saturating_add(1), text, viewer))
                .collect()
        }
    } else {
        vec![Line::raw("Select a file to view it.")]
    };
    frame.render_widget(
        Paragraph::new(lines).scroll((
            0,
            model
                .viewer_horizontal_scroll
                .try_into()
                .unwrap_or(u16::MAX),
        )),
        inner,
    );
}

fn viewer_line(number: usize, text: &str, viewer: &super::model::Viewer) -> Line<'static> {
    let marker = viewer.markers.get(&number).copied();
    let marker_text = if marker.is_some() { "▌" } else { " " };
    let mut spans = vec![
        Span::styled(
            format!("{number:>4} "),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(marker_text, marker_style(marker)),
        Span::raw(" "),
    ];
    let number = u32::try_from(number).unwrap_or(u32::MAX);
    if let Some(highlighted) = viewer.highlighted.get(&number) {
        spans.extend(plain_syntax_spans(highlighted));
    } else {
        spans.push(Span::raw(text.to_owned()));
    }
    Line::from(spans)
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
}
