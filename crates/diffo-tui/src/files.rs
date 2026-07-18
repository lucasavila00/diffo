use super::{
    Alignment, Block, Borders, ChangeArea, ChangeKind, Color, Constraint, Direction,
    FileListScroll, FileState, Frame, HighlightSpacing, Layout, Line, List, ListItem, ListState,
    Model, Modifier, Paragraph, Rect, RepositorySnapshot, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Span, Style, file_action_style, file_kind_style, horizontal_panes, main_area,
    network_animation_style,
};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct FileListMetrics {
    pub(super) staged: FileGroupMetrics,
    pub(super) unstaged: FileGroupMetrics,
}

impl FileListMetrics {
    pub(super) const fn get(self, area: ChangeArea) -> FileGroupMetrics {
        match area {
            ChangeArea::Staged => self.staged,
            ChangeArea::Unstaged => self.unstaged,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct FileGroupMetrics {
    pub(super) list_area: Rect,
    pub(super) scrollbar_area: Rect,
    pub(super) maximum_scroll: usize,
    pub(super) offset: usize,
}

pub(super) fn render_files(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    model: &Model,
) -> FileListMetrics {
    let panels = file_panel_areas(area);
    render_commit_composer(frame, panels[0], model);
    let groups = file_group_areas(panels[1]);
    let staged = render_file_group(
        frame,
        groups[0],
        " Staged [-] Unstage All ",
        staged_files(&model.snapshot),
        ChangeArea::Staged,
        model,
    );
    let unstaged = render_file_group(
        frame,
        groups[1],
        " Changes [+] Stage All ",
        unstaged_files(&model.snapshot),
        ChangeArea::Unstaged,
        model,
    );
    FileListMetrics { staged, unstaged }
}

pub(super) fn file_panel_areas(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::vertical([Constraint::Length(6), Constraint::Min(2)]).split(area)
}

pub(super) fn commit_composer_areas(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .split(area)
}

pub(super) fn render_commit_composer(frame: &mut Frame, area: Rect, model: &Model) {
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
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Commit message · click to edit "),
            ),
        sections[0],
    );
    let action = model.primary_action();
    let style = if model.primary_action_enabled() {
        Style::default()
            .bg(Color::Indexed(24))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
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
) -> Option<diffo_app::Message> {
    let columns = horizontal_panes(main_area(area), model.file_pane_percent);
    let file_areas = file_panel_areas(columns[0]);
    let sections = commit_composer_areas(file_areas[0]);
    if sections[0].contains((column, row).into()) {
        return Some(diffo_app::Message::FocusCommitInput);
    }
    if sections[1].contains((column, row).into())
        && (model.primary_action_enabled()
            || model.primary_action() == diffo_app::PrimaryAction::PushAndPull)
    {
        return Some(diffo_app::Message::ExecutePrimaryAction);
    }
    None
}

pub(super) fn file_group_areas(
    area: ratatui::layout::Rect,
) -> std::rc::Rc<[ratatui::layout::Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area)
}

pub(super) fn render_file_group<'a>(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    title: &str,
    files: impl Iterator<Item = &'a FileState>,
    change_area: ChangeArea,
    model: &Model,
) -> FileGroupMetrics {
    let files = files.collect::<Vec<_>>();
    let metrics = file_group_metrics(area, files.len(), model.file_list_scroll.get(change_area));
    let selected = files
        .iter()
        .position(|file| model.is_selected(&file.path, change_area))
        .filter(|selected| {
            *selected >= metrics.offset
                && *selected
                    < metrics
                        .offset
                        .saturating_add(usize::from(metrics.list_area.height))
        })
        .map(|selected| selected - metrics.offset);
    let items = files
        .into_iter()
        .skip(metrics.offset)
        .take(usize::from(metrics.list_area.height))
        .map(|file| {
            file_item(
                file,
                model.is_selected(&file.path, change_area),
                change_area,
                usize::from(metrics.list_area.width.saturating_sub(2)),
            )
        });
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(resize_border_style(model))
            .title(title),
        area,
    );
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ")
        .highlight_spacing(HighlightSpacing::Always);
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(list, metrics.list_area, &mut state);
    if metrics.maximum_scroll > 0 && !metrics.scrollbar_area.is_empty() {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(Style::default().fg(Color::Cyan));
        let mut state = ScrollbarState::new(metrics.maximum_scroll.saturating_add(1))
            .viewport_content_length(usize::from(metrics.list_area.height))
            .position(metrics.offset);
        frame.render_stateful_widget(scrollbar, metrics.scrollbar_area, &mut state);
    }
    metrics
}

pub(super) fn file_group_metrics(
    area: Rect,
    file_count: usize,
    requested_offset: usize,
) -> FileGroupMetrics {
    let content = area.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    let viewport_rows = usize::from(content.height);
    let maximum_scroll = if viewport_rows == 0 {
        0
    } else {
        file_count.saturating_sub(viewport_rows)
    };
    let offset = requested_offset.min(maximum_scroll);
    let has_scrollbar = maximum_scroll > 0 && content.width > 0;
    let list_area = Rect::new(
        content.x,
        content.y,
        content.width.saturating_sub(u16::from(has_scrollbar)),
        content.height,
    );
    let scrollbar_area = if has_scrollbar {
        Rect::new(
            content.right().saturating_sub(1),
            content.y,
            1,
            content.height,
        )
    } else {
        Rect::default()
    };
    FileGroupMetrics {
        list_area,
        scrollbar_area,
        maximum_scroll,
        offset,
    }
}

pub(super) fn prepared_file_list_scroll(model: &Model, area: Rect) -> FileListScroll {
    let file_pane = horizontal_panes(main_area(area), model.file_pane_percent)[0];
    let panels = file_panel_areas(file_pane);
    let groups = file_group_areas(panels[1]);
    let staged_count = staged_files(&model.snapshot).count();
    let unstaged_count = unstaged_files(&model.snapshot).count();
    FileListScroll {
        staged: file_group_metrics(groups[0], staged_count, model.file_list_scroll.staged).offset,
        unstaged: file_group_metrics(groups[1], unstaged_count, model.file_list_scroll.unstaged)
            .offset,
    }
}

pub(super) fn file_item(
    file: &FileState,
    selected: bool,
    change_area: ChangeArea,
    width: usize,
) -> ListItem<'static> {
    let marker = match file.kind {
        ChangeKind::Added | ChangeKind::Untracked => "A",
        ChangeKind::Modified => "M",
        ChangeKind::Deleted => "D",
        ChangeKind::Renamed => "R",
        ChangeKind::Copied => "C",
        ChangeKind::Conflicted => "U",
    };
    let label = format!("{marker}  {}", file.path.display());
    let style = file_kind_style(file.kind, selected);
    if width < 3 {
        return ListItem::new(Line::styled(label, style));
    }
    let action = match change_area {
        ChangeArea::Staged => "[-]",
        ChangeArea::Unstaged => "[+]",
    };
    let label_width = width.saturating_sub(action.len());
    let mut label = label.chars().take(label_width).collect::<String>();
    label.push_str(&" ".repeat(label_width.saturating_sub(label.chars().count())));
    ListItem::new(Line::from(vec![
        Span::styled(label, style),
        Span::styled(action, file_action_style(change_area)),
    ]))
}

pub(super) fn render_status(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    model: &Model,
    animation_tick: usize,
) {
    let text = if let Some(operation) = model.network_operation() {
        const SPINNER: [&str; 4] = ["◐", "◓", "◑", "◒"];
        format!(
            " {} {}… · Ctrl+C to exit ",
            SPINNER[(animation_tick / 2) % SPINNER.len()],
            operation.label()
        )
    } else if let Some(error) = model.error.as_deref() {
        error.to_owned()
    } else if model.resizing_file_pane {
        format!(
            " Resizing file pane: {}% · release mouse to finish ",
            model.file_pane_percent
        )
    } else {
        " 1/f1: commands  2/f2: help ".to_owned()
    };
    let style = if model.network_operation().is_some() {
        network_animation_style(animation_tick)
    } else if model.error.is_some() {
        Style::default().fg(Color::Red)
    } else if model.resizing_file_pane {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}

pub(super) fn resize_border_style(model: &Model) -> Style {
    if model.resizing_file_pane {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

pub(super) fn unstaged_files(snapshot: &RepositorySnapshot) -> impl Iterator<Item = &FileState> {
    snapshot
        .files
        .iter()
        .filter(|file| file.unstaged.is_some() || file.kind == ChangeKind::Untracked)
}

pub(super) fn staged_files(snapshot: &RepositorySnapshot) -> impl Iterator<Item = &FileState> {
    snapshot.files.iter().filter(|file| file.staged.is_some())
}
