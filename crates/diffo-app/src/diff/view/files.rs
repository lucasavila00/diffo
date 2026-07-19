use crate::diff::{
    Alignment, Block, Borders, ChangeArea, ChangeKind, Constraint, Direction, FileKey, FileState,
    Frame, HeadState, Layout, Line, Model, Modifier, Paragraph, Rect, RepositorySnapshot, Span,
    Style, change_kind_style, horizontal_panes, main_area, terminal_safe_text,
};
use diffo_ui::file_picker::{Document, Row as PickerRow};
use diffo_ui::{design, disabled_control_style, enabled_control_style, interaction, theme};

pub(in crate::diff) fn file_panel_areas(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::vertical([
        Constraint::Length(design::FILE_COMPOSER_HEIGHT),
        Constraint::Min(design::MIN_FILE_GROUP_HEIGHT),
    ])
    .split(area)
}

pub(in crate::diff) fn commit_composer_areas(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::vertical([
        Constraint::Length(design::COMMIT_FIELD_HEIGHT),
        Constraint::Length(design::PRIMARY_ACTION_HEIGHT),
        Constraint::Length(design::STATUS_HEIGHT),
    ])
    .split(area)
}

pub(in crate::diff) fn render_commit_composer(frame: &mut Frame, area: Rect, model: &Model) {
    let sections = commit_composer_areas(area);
    let empty = model.commit_message.is_empty();
    let message = if empty {
        model
            .suggested_commit_message()
            .unwrap_or_else(|| "Type a message…".to_owned())
    } else {
        model.commit_message.clone()
    };
    frame.render_widget(
        Paragraph::new(message)
            .style(if empty {
                Style::default().fg(theme::CHROME)
            } else {
                Style::default().fg(theme::TEXT)
            })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(resize_border_style(model))
                    .title(" Commit message ")
                    .title(
                        Line::styled(interaction::EDIT, enabled_control_style())
                            .alignment(Alignment::Right),
                    ),
            ),
        sections[0],
    );
    let action = model.primary_action();
    let style = if primary_action_clickable(model) {
        enabled_control_style().bg(theme::SELECTION_BACKGROUND)
    } else {
        disabled_control_style()
    };
    frame.render_widget(
        Paragraph::new(format!("[ {} ]", action.label()))
            .alignment(Alignment::Center)
            .style(style),
        sections[1],
    );
}

pub(crate) fn commit_action_at_position(
    model: &Model,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<crate::diff::Message> {
    let columns = horizontal_panes(main_area(area), model.file_pane_percent);
    let file_areas = file_panel_areas(columns[0]);
    let sections = commit_composer_areas(file_areas[0]);
    if sections[0].contains((column, row).into()) {
        return Some(crate::diff::Message::FocusCommitInput);
    }
    if sections[1].contains((column, row).into()) && primary_action_clickable(model) {
        return Some(crate::diff::Message::ExecutePrimaryAction);
    }
    None
}

fn primary_action_clickable(model: &Model) -> bool {
    model.primary_action_enabled()
        || model.primary_action() == crate::diff::PrimaryAction::PushAndPull
}

pub(in crate::diff) fn file_group_areas(
    area: ratatui::layout::Rect,
) -> std::rc::Rc<[ratatui::layout::Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(design::EQUAL_SPLIT_PERCENT),
            Constraint::Percentage(design::EQUAL_SPLIT_PERCENT),
        ])
        .split(area)
}

pub(in crate::diff) fn picker_document<'a>(
    title: &str,
    panel_action: &str,
    files: impl Iterator<Item = &'a FileState>,
    change_area: ChangeArea,
    border_style: Style,
) -> Document<FileKey> {
    let rows = files
        .map(|file| {
            let key = FileKey {
                path: file.path.clone(),
                area: change_area,
            };
            let action = match change_area {
                ChangeArea::Staged => "[-]",
                ChangeArea::Unstaged => "[+]",
            };
            PickerRow::flat(key, file_label(file)).with_action(action)
        })
        .collect();
    let mut document = Document::flat(title, rows);
    document.panel_action = Some(panel_action.to_owned());
    document.border_style = border_style;
    document
}

pub(in crate::diff) fn file_label(file: &FileState) -> Line<'static> {
    let marker = match file.kind {
        ChangeKind::Added | ChangeKind::Untracked => "A",
        ChangeKind::Modified => "M",
        ChangeKind::Deleted => "D",
        ChangeKind::Renamed => "R",
        ChangeKind::Copied => "C",
        ChangeKind::Conflicted => "U",
    };
    Line::styled(
        terminal_safe_text(&format!("{marker}  {}", file.path.display())),
        change_kind_style(file.kind, false),
    )
}

pub(in crate::diff) fn render_status(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    model: &Model,
) {
    frame.render_widget(
        Paragraph::new(status_line(model, 0, usize::from(area.width))),
        area,
    );
}

pub(in crate::diff) fn status_line(
    model: &Model,
    animation_tick: usize,
    width: usize,
) -> Line<'static> {
    let mut head = Span::styled(head_label(&model.snapshot.head), head_style());
    let status = repository_status(&model.snapshot);
    let mut status = Some(Span::styled(
        format!(" · {}", status.label()),
        status.style(),
    ));
    let mut divergence = model.snapshot.upstream.as_ref().and_then(|upstream| {
        (upstream.ahead != 0 || upstream.behind != 0).then(|| {
            Span::styled(
                format!(" · ↓{} ↑{}", upstream.behind, upstream.ahead),
                Style::default().fg(theme::TEXT),
            )
        })
    });
    let mut transient = transient_status(model, animation_tick);
    let mut help = Some(Span::raw("1/f1: commands  2/f2: help "));

    while status_width(
        &head,
        status.as_ref(),
        divergence.as_ref(),
        transient.as_ref(),
        help.as_ref(),
    ) > width
    {
        if divergence.take().is_some() {
            continue;
        }
        if status.take().is_some() {
            continue;
        }
        if help.take().is_some() {
            continue;
        }
        if let Some(message) = transient.as_mut() {
            let available =
                width.saturating_sub(head.width().saturating_add(usize::from(design::INLINE_GAP)));
            if available == 0 {
                transient = None;
            } else {
                message.content = truncate_width(message.content.as_ref(), available).into();
            }
            continue;
        }
        head.content = truncate_width(head.content.as_ref(), width).into();
        break;
    }

    let mut spans = vec![head];
    spans.extend(status);
    spans.extend(divergence);
    let left_width = spans.iter().map(Span::width).sum::<usize>();
    let transient_width = transient.as_ref().map_or(0, Span::width);
    let help_width = help.as_ref().map_or(0, Span::width);

    if let Some(transient) = transient {
        spans.push(Span::raw(" ".repeat(usize::from(design::INLINE_GAP))));
        spans.push(transient);
    }
    if let Some(help) = help {
        let used = left_width
            .saturating_add(if transient_width == 0 {
                0
            } else {
                usize::from(design::INLINE_GAP)
            })
            .saturating_add(transient_width);
        spans.push(Span::raw(
            " ".repeat(width.saturating_sub(used.saturating_add(help_width))),
        ));
        spans.push(help);
    }
    Line::from(spans)
}

fn transient_status(model: &Model, _animation_tick: usize) -> Option<Span<'static>> {
    if let Some(error) = model.error.as_deref() {
        Some(Span::styled(
            terminal_safe_text(error),
            Style::default().fg(theme::DANGER),
        ))
    } else if model.resizing_file_pane {
        Some(Span::styled(
            format!(
                "Resizing file pane: {}% · release mouse to finish",
                model.file_pane_percent
            ),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        None
    }
}

#[derive(Clone, Copy)]
enum RepositoryStatus {
    Conflicts,
    Staged,
    Changes,
    Clean,
}

impl RepositoryStatus {
    const fn label(self) -> &'static str {
        match self {
            Self::Conflicts => "conflicts",
            Self::Staged => "staged",
            Self::Changes => "changes",
            Self::Clean => "clean",
        }
    }

    fn style(self) -> Style {
        match self {
            Self::Conflicts => Style::default()
                .fg(theme::DANGER)
                .add_modifier(Modifier::BOLD),
            Self::Staged => Style::default()
                .fg(theme::SUCCESS)
                .add_modifier(Modifier::BOLD),
            Self::Changes => Style::default().fg(theme::WARNING),
            Self::Clean => Style::default().fg(theme::CHROME),
        }
    }
}

fn repository_status(snapshot: &RepositorySnapshot) -> RepositoryStatus {
    if snapshot
        .files
        .iter()
        .any(|file| file.kind == ChangeKind::Conflicted)
    {
        RepositoryStatus::Conflicts
    } else if snapshot.files.iter().any(|file| file.staged.is_some()) {
        RepositoryStatus::Staged
    } else if snapshot
        .files
        .iter()
        .any(|file| file.unstaged.is_some() || file.kind == ChangeKind::Untracked)
    {
        RepositoryStatus::Changes
    } else {
        RepositoryStatus::Clean
    }
}

fn head_label(head: &HeadState) -> String {
    match head {
        HeadState::Named { name, .. } => format!(" branch {name}"),
        HeadState::Unborn { name } => format!(" branch {name} (unborn)"),
        HeadState::Detached { commit } => format!(" detached {}", short_commit(commit)),
    }
}

fn short_commit(commit: &str) -> String {
    commit.chars().take(7).collect()
}

fn head_style() -> Style {
    Style::default()
        .fg(theme::TEXT)
        .add_modifier(Modifier::BOLD)
}

fn status_width(
    head: &Span<'_>,
    status: Option<&Span<'_>>,
    divergence: Option<&Span<'_>>,
    transient: Option<&Span<'_>>,
    help: Option<&Span<'_>>,
) -> usize {
    head.width()
        .saturating_add(status.map_or(0, Span::width))
        .saturating_add(divergence.map_or(0, Span::width))
        .saturating_add(transient.map_or(0, |span| {
            span.width().saturating_add(usize::from(design::INLINE_GAP))
        }))
        .saturating_add(help.map_or(0, Span::width))
}

fn truncate_width(value: &str, width: usize) -> String {
    if Span::raw(value).width() <= width {
        return value.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let content_width = width - 1;
    let mut result = String::new();
    let mut used = 0_usize;
    for character in value.chars() {
        let character_width = Span::raw(character.to_string()).width();
        if used.saturating_add(character_width) > content_width {
            break;
        }
        result.push(character);
        used = used.saturating_add(character_width);
    }
    result.push('…');
    result
}

pub(in crate::diff) fn resize_border_style(model: &Model) -> Style {
    let style = Style::default().fg(theme::CHROME);
    if model.resizing_file_pane {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

pub(in crate::diff) fn unstaged_files(
    snapshot: &RepositorySnapshot,
) -> impl Iterator<Item = &FileState> {
    snapshot
        .files
        .iter()
        .filter(|file| file.unstaged.is_some() || file.kind == ChangeKind::Untracked)
}

pub(in crate::diff) fn staged_files(
    snapshot: &RepositorySnapshot,
) -> impl Iterator<Item = &FileState> {
    snapshot.files.iter().filter(|file| file.staged.is_some())
}
