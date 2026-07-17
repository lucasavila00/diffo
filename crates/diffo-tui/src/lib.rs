use diffo_app::{ChangeArea, DiffViewMode, FileKey, Model};
use std::path::{Path, PathBuf};

use diffo_core::{AccessMode, ChangeKind, FileState, RepositorySnapshot};
use diffo_diff::{
    RenderLine, RowKind, SideBySideRow, inline_rows, parse_unified_patch, side_by_side_rows,
};
use diffo_highlight::{HighlightedDiff, HighlightedLine, StyledSpan, SyntaxHighlighter};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

mod input;

pub use input::map_event;

pub struct Renderer {
    highlighter: SyntaxHighlighter,
    highlighted: Option<HighlightCache>,
    #[cfg(test)]
    highlight_computations: usize,
}

struct HighlightCache {
    path: PathBuf,
    patch: String,
    highlighted: HighlightedDiff,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            highlighter: SyntaxHighlighter::new(),
            highlighted: None,
            #[cfg(test)]
            highlight_computations: 0,
        }
    }

    pub fn render(&mut self, frame: &mut Frame, model: &Model) {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(frame.area());
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(vertical[0]);

        render_files(frame, panes[0], model);
        self.render_diff(frame, panes[1], model);
        render_status(frame, vertical[1], model);
    }

    fn highlights<'a>(
        &'a mut self,
        path: &Path,
        source_patch: &str,
        document: &diffo_diff::DiffDocument,
    ) -> &'a HighlightedDiff {
        let cache_matches = self
            .highlighted
            .as_ref()
            .is_some_and(|cache| cache.path == path && cache.patch == source_patch);
        if !cache_matches {
            self.highlighted = Some(HighlightCache {
                path: path.to_path_buf(),
                patch: source_patch.to_owned(),
                highlighted: self.highlighter.highlight(path, document),
            });
            #[cfg(test)]
            {
                self.highlight_computations += 1;
            }
        }
        &self
            .highlighted
            .as_ref()
            .expect("cache was populated")
            .highlighted
    }

    fn render_diff(&mut self, frame: &mut Frame, area: ratatui::layout::Rect, model: &Model) {
        let mode = match model.diff_view_mode {
            DiffViewMode::Inline => "Inline",
            DiffViewMode::SideBySide => "Side by side",
        };
        let lines = self.diff_lines(model, area.width.saturating_sub(2));
        let pane = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" File Diff · {mode} ")),
            )
            .scroll((
                model.diff_scroll.try_into().unwrap_or(u16::MAX),
                model.diff_horizontal_scroll.try_into().unwrap_or(u16::MAX),
            ));
        frame.render_widget(pane, area);
    }

    fn diff_lines(&mut self, model: &Model, width: u16) -> Vec<Line<'static>> {
        let Some(selected) = model.selected.as_ref() else {
            return vec![Line::raw("No file selected.")];
        };
        let Some(file) = model
            .snapshot
            .files
            .iter()
            .find(|file| file.path == selected.path)
        else {
            return vec![Line::raw("Selected file is no longer available.")];
        };
        let diff = match selected.area {
            ChangeArea::Unstaged => file.unstaged.as_ref(),
            ChangeArea::Staged => file.staged.as_ref(),
        };
        let Some(diff) = diff else {
            return vec![Line::raw("No text diff is available for this file.")];
        };
        let Ok(document) = parse_unified_patch(&diff.text) else {
            return diff
                .text
                .lines()
                .map(|line| Line::raw(line.to_owned()))
                .collect();
        };
        if document.binary {
            return vec![Line::raw("Binary file changed.")];
        }
        let highlighted = self.highlights(&file.path, &diff.text, &document);

        match model.diff_view_mode {
            DiffViewMode::Inline => inline_rows(&document)
                .iter()
                .map(|row| inline_line(row, highlighted))
                .collect(),
            DiffViewMode::SideBySide => {
                let column_width = usize::from(width.saturating_sub(3) / 2);
                side_by_side_rows(&document)
                    .iter()
                    .map(|row| side_by_side_line(row, column_width, highlighted))
                    .collect()
            }
        }
    }
}

pub(crate) fn file_at_position(
    model: &Model,
    area: ratatui::layout::Rect,
    column: u16,
    row: u16,
) -> Option<FileKey> {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(vertical[0]);
    let files = panes[0];
    let inside_files = column > files.x
        && column < files.x.saturating_add(files.width).saturating_sub(1)
        && row > files.y
        && row < files.y.saturating_add(files.height).saturating_sub(1);
    if !inside_files {
        return None;
    }

    file_at_display_row(model, usize::from(row - files.y - 1))
}

fn file_at_display_row(model: &Model, row: usize) -> Option<FileKey> {
    let unstaged = unstaged_files(&model.snapshot).collect::<Vec<_>>();
    if (1..=unstaged.len()).contains(&row) {
        return Some(FileKey {
            path: unstaged[row - 1].path.clone(),
            area: ChangeArea::Unstaged,
        });
    }
    let staged_index = row.checked_sub(unstaged.len() + 2)?;
    staged_files(&model.snapshot)
        .nth(staged_index)
        .map(|file| FileKey {
            path: file.path.clone(),
            area: ChangeArea::Staged,
        })
}

fn render_files(frame: &mut Frame, area: ratatui::layout::Rect, model: &Model) {
    let mut items = vec![group_header("Changes")];
    items.extend(
        unstaged_files(&model.snapshot)
            .map(|file| file_item(file, model.is_selected(&file.path, ChangeArea::Unstaged))),
    );
    items.push(group_header("Staged Changes"));
    items.extend(
        staged_files(&model.snapshot)
            .map(|file| file_item(file, model.is_selected(&file.path, ChangeArea::Staged))),
    );

    let title = if model.access_mode == AccessMode::ReadOnly {
        " Files · read-only "
    } else {
        " Files · [a] Stage All "
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    let mut state = ListState::default().with_selected(model.selected_row());
    frame.render_stateful_widget(list, area, &mut state);
}

fn group_header(name: &str) -> ListItem<'_> {
    ListItem::new(Line::styled(
        name,
        Style::default().add_modifier(Modifier::BOLD),
    ))
}

fn file_item(file: &FileState, selected: bool) -> ListItem<'static> {
    let marker = match file.kind {
        ChangeKind::Added | ChangeKind::Untracked => "A",
        ChangeKind::Modified => "M",
        ChangeKind::Deleted => "D",
        ChangeKind::Renamed => "R",
        ChangeKind::Copied => "C",
        ChangeKind::Conflicted => "U",
    };
    let line = format!("{marker}  {}", file.path.display());
    let style = if selected {
        Style::default().fg(Color::White)
    } else {
        Style::default()
    };
    ListItem::new(Line::styled(line, style))
}

fn inline_line(row: &RenderLine, highlighted: &HighlightedDiff) -> Line<'static> {
    let prefix = match row.kind {
        RowKind::Removed => "-",
        RowKind::Added => "+",
        RowKind::Header => "@",
        _ => " ",
    };
    let number = row
        .number
        .map_or_else(|| "    ".to_owned(), |number| format!("{number:>4}"));
    if matches!(row.kind, RowKind::Header | RowKind::Meta) {
        return Line::styled(
            format!("{number} {prefix} {}", row.text),
            row_style(row.kind),
        );
    }
    let mut spans = vec![Span::styled(
        format!("{number} {prefix} "),
        gutter_style(row.kind),
    )];
    spans.extend(code_spans(row, highlighted));
    Line::from(spans)
}

fn side_by_side_line(
    row: &SideBySideRow,
    column_width: usize,
    highlighted: &HighlightedDiff,
) -> Line<'static> {
    let mut spans = format_cell(row.old.as_ref(), column_width, highlighted);
    spans.push(Span::raw(" │ "));
    spans.extend(format_cell(row.new.as_ref(), column_width, highlighted));
    Line::from(spans)
}

fn format_cell(
    line: Option<&RenderLine>,
    width: usize,
    highlighted: &HighlightedDiff,
) -> Vec<Span<'static>> {
    let Some(line) = line else {
        return vec![Span::raw(" ".repeat(width))];
    };
    let number = line
        .number
        .map_or_else(|| "    ".to_owned(), |number| format!("{number:>4}"));
    if matches!(line.kind, RowKind::Header | RowKind::Meta) {
        return clip_and_pad(
            vec![Span::styled(
                format!("{number} {}", line.text),
                row_style(line.kind),
            )],
            width,
            Style::default(),
        );
    }
    let mut spans = vec![Span::styled(format!("{number} "), gutter_style(line.kind))];
    spans.extend(code_spans(line, highlighted));
    clip_and_pad(spans, width, diff_background(line.kind))
}

fn code_spans(row: &RenderLine, highlighted: &HighlightedDiff) -> Vec<Span<'static>> {
    let highlighted_line = row.number.and_then(|number| match row.kind {
        RowKind::Removed => highlighted.old.get(&number),
        RowKind::Added | RowKind::Context | RowKind::Changed => highlighted.new.get(&number),
        RowKind::Header | RowKind::Meta => None,
    });
    let background = diff_background(row.kind);
    highlighted_line.map_or_else(
        || vec![Span::styled(row.text.clone(), background)],
        |line| syntax_spans(line, background),
    )
}

fn syntax_spans(line: &HighlightedLine, background: Style) -> Vec<Span<'static>> {
    line.spans
        .iter()
        .map(|span| Span::styled(span.text.clone(), syntax_style(span).patch(background)))
        .collect()
}

fn syntax_style(span: &StyledSpan) -> Style {
    let mut style = Style::default().fg(Color::Rgb(
        span.foreground.red,
        span.foreground.green,
        span.foreground.blue,
    ));
    if span.bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if span.italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if span.underline {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    style
}

fn clip_and_pad(
    spans: Vec<Span<'static>>,
    width: usize,
    padding_style: Style,
) -> Vec<Span<'static>> {
    let mut remaining = width;
    let mut clipped = Vec::new();
    for span in spans {
        if remaining == 0 {
            break;
        }
        let text = span.content.chars().take(remaining).collect::<String>();
        remaining = remaining.saturating_sub(text.chars().count());
        clipped.push(Span::styled(text, span.style));
    }
    if remaining > 0 {
        clipped.push(Span::styled(" ".repeat(remaining), padding_style));
    }
    clipped
}

fn gutter_style(kind: RowKind) -> Style {
    Style::default()
        .fg(Color::DarkGray)
        .patch(diff_background(kind))
}

fn diff_background(kind: RowKind) -> Style {
    match kind {
        RowKind::Removed => Style::default().bg(Color::Rgb(55, 30, 35)),
        RowKind::Added => Style::default().bg(Color::Rgb(25, 50, 35)),
        RowKind::Header | RowKind::Context | RowKind::Changed | RowKind::Meta => Style::default(),
    }
}

fn row_style(kind: RowKind) -> Style {
    match kind {
        RowKind::Header => Style::default().fg(Color::Cyan),
        RowKind::Removed => Style::default().fg(Color::Red),
        RowKind::Added => Style::default().fg(Color::Green),
        RowKind::Meta => Style::default().fg(Color::Yellow),
        RowKind::Context | RowKind::Changed => Style::default(),
    }
}

fn render_status(frame: &mut Frame, area: ratatui::layout::Rect, model: &Model) {
    let text = model
        .error
        .as_deref()
        .unwrap_or(if model.access_mode == AccessMode::ReadOnly {
            input::READ_ONLY_HELP
        } else {
            input::READ_WRITE_HELP
        });
    let style = if model.error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}

fn unstaged_files(snapshot: &RepositorySnapshot) -> impl Iterator<Item = &FileState> {
    snapshot
        .files
        .iter()
        .filter(|file| file.unstaged.is_some() || file.kind == ChangeKind::Untracked)
}

fn staged_files(snapshot: &RepositorySnapshot) -> impl Iterator<Item = &FileState> {
    snapshot.files.iter().filter(|file| file.staged.is_some())
}

#[cfg(test)]
mod rendering_tests {
    use std::path::PathBuf;

    use diffo_app::{DiffViewMode, Model};
    use diffo_core::{AccessMode, ChangeKind, FileDiff, FileState, RepositorySnapshot};
    use ratatui::style::Color;

    use super::Renderer;

    fn model() -> Model {
        Model::new(
            RepositorySnapshot {
                files: vec![FileState {
                    path: PathBuf::from("src/main.rs"),
                    old_path: None,
                    kind: ChangeKind::Modified,
                    staged: None,
                    unstaged: Some(FileDiff {
                        text: "@@ -1 +1 @@\n-let old = true;\n+let new = false;\n".to_owned(),
                    }),
                }],
                ..RepositorySnapshot::default()
            },
            AccessMode::ReadWrite,
        )
    }

    #[test]
    fn renders_syntax_foregrounds_over_diff_backgrounds() {
        let mut renderer = Renderer::new();
        let lines = renderer.diff_lines(&model(), 80);
        let removed = &lines[1];
        let added = &lines[2];

        assert!(removed.spans.iter().any(|span| span.style.fg.is_some()));
        assert!(
            removed
                .spans
                .iter()
                .any(|span| { span.style.bg == Some(Color::Rgb(55, 30, 35)) })
        );
        assert!(
            added
                .spans
                .iter()
                .any(|span| { span.style.bg == Some(Color::Rgb(25, 50, 35)) })
        );
    }

    #[test]
    fn reuses_highlights_across_modes_and_invalidates_changed_patch() {
        let mut renderer = Renderer::new();
        let mut model = model();

        renderer.diff_lines(&model, 80);
        model.diff_view_mode = DiffViewMode::SideBySide;
        renderer.diff_lines(&model, 80);
        assert_eq!(renderer.highlight_computations, 1);

        model.snapshot.files[0]
            .unstaged
            .as_mut()
            .expect("unstaged diff")
            .text
            .push_str("\\ No newline at end of file\n");
        renderer.diff_lines(&model, 80);
        assert_eq!(renderer.highlight_computations, 2);
    }
}
