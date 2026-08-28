use diffo_core::{ChangeKind, Commit, CommitFile};
use diffo_ui::{
    PaneSplit, change_kind_style, file_icons,
    file_picker::{Document, Row},
    terminal_safe_text, theme, tool_areas,
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
};

use super::HistoryTarget;

#[derive(Clone, Copy)]
pub(super) struct HistoryAreas {
    pub(super) commits: Rect,
    pub(super) files: Rect,
    pub(super) review: Rect,
}

pub(super) fn areas(area: Rect, split: PaneSplit) -> HistoryAreas {
    let panes = split.areas(tool_areas(area).content);
    let leading = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(panes.leading);
    HistoryAreas {
        commits: leading[0],
        files: leading[1],
        review: panes.trailing,
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

pub(super) fn file_document(files: &[CommitFile], border_style: Style) -> Document<HistoryTarget> {
    let rows = files
        .iter()
        .map(|file| {
            let marker = match file.kind {
                ChangeKind::Added | ChangeKind::Untracked => "A",
                ChangeKind::Modified => "M",
                ChangeKind::Deleted => "D",
                ChangeKind::Renamed => "R",
                ChangeKind::Copied => "C",
                ChangeKind::Conflicted => "U",
            };
            Row::flat(
                HistoryTarget::File(file.path.clone()),
                Line::styled(
                    terminal_safe_text(&format!(
                        "{marker} {}{}",
                        file_icons::file_icon(&file.path),
                        file.path.display()
                    )),
                    change_kind_style(file.kind),
                ),
            )
        })
        .collect();
    let mut document = Document::flat("Files", rows);
    document.border_style = border_style;
    "No files in this commit.".clone_into(&mut document.empty_message);
    document
}

pub(super) fn file_title(file: &CommitFile) -> Line<'static> {
    Line::styled(
        terminal_safe_text(&format!(
            " {}{} ",
            file_icons::file_icon(&file.path),
            file.path.display()
        )),
        change_kind_style(file.kind),
    )
}
