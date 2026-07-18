#![doc = include_str!("../README.md")]

use std::collections::HashSet;
use std::hash::Hash;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use diffo_ui::interaction;
use diffo_ui::{
    design, enabled_control_style, maximum_scroll, scroll_offset, scrollbar_position,
    scrollbar_position_count, terminal_safe_text, theme, wheel_scroll_delta,
};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Flat,
    Tree,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Navigation {
    Previous,
    Next,
    First,
    Last,
    Activate,
    OpenMenu,
}

#[derive(Clone, Debug)]
pub struct Row<K> {
    pub id: K,
    pub label: Line<'static>,
    pub depth: usize,
    pub branch: bool,
    pub action: Option<String>,
    pub context_menu: bool,
}

impl<K> Row<K> {
    pub fn flat(id: K, label: Line<'static>) -> Self {
        Self {
            id,
            label,
            depth: 0,
            branch: false,
            action: None,
            context_menu: true,
        }
    }

    pub fn tree(id: K, label: Line<'static>, depth: usize, branch: bool) -> Self {
        Self {
            id,
            label,
            depth,
            branch,
            action: None,
            context_menu: !branch,
        }
    }

    #[must_use]
    pub fn with_action(mut self, label: impl Into<String>) -> Self {
        self.action = Some(label.into());
        self
    }
}

#[derive(Clone, Debug)]
pub struct Document<K> {
    pub title: String,
    pub mode: Mode,
    pub rows: Vec<Row<K>>,
    pub panel_action: Option<String>,
    pub empty_message: String,
    pub border_style: Style,
}

impl<K> Document<K> {
    pub fn flat(title: impl Into<String>, rows: Vec<Row<K>>) -> Self {
        Self {
            title: title.into(),
            mode: Mode::Flat,
            rows,
            panel_action: None,
            empty_message: "No files.".to_owned(),
            border_style: Style::default().fg(theme::CHROME),
        }
    }

    pub fn tree(title: impl Into<String>, rows: Vec<Row<K>>) -> Self {
        Self {
            title: title.into(),
            mode: Mode::Tree,
            rows,
            panel_action: None,
            empty_message: "No files.".to_owned(),
            border_style: Style::default().fg(theme::CHROME),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome<K> {
    Consumed,
    Selected(K),
    Activated(K),
    RowAction(K),
    PanelAction,
    CopyPath { id: K, absolute: bool },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Metrics {
    pub list_area: Rect,
    pub scrollbar_area: Rect,
    pub maximum_offset: usize,
    pub offset: usize,
}

#[derive(Clone, Debug)]
struct ContextMenu<K> {
    id: K,
    column: u16,
    row: u16,
}

pub struct FilePicker<K> {
    document: Document<K>,
    visible: Vec<usize>,
    selected: Option<K>,
    expanded: HashSet<K>,
    area: Rect,
    metrics: Metrics,
    offset: usize,
    dragging_scrollbar: bool,
    context_menu: Option<ContextMenu<K>>,
}

impl<K> Default for FilePicker<K> {
    fn default() -> Self {
        Self {
            document: Document::flat(String::new(), Vec::new()),
            visible: Vec::new(),
            selected: None,
            expanded: HashSet::new(),
            area: Rect::default(),
            metrics: Metrics::default(),
            offset: 0,
            dragging_scrollbar: false,
            context_menu: None,
        }
    }
}

impl<K> FilePicker<K>
where
    K: Clone + Eq + Hash,
{
    pub fn prepare(&mut self, area: Rect, document: Document<K>, requested_selection: Option<&K>) {
        let selected_before = self.selected.clone();
        let old_selected_index = self.selected_visible_index();
        self.area = area;
        self.expanded
            .retain(|id| document.rows.iter().any(|row| row.branch && &row.id == id));
        self.document = document;
        self.rebuild_visible();

        let requested = requested_selection.filter(|id| self.visible_contains(id));
        let preserved = self
            .selected
            .as_ref()
            .filter(|id| self.visible_contains(id));
        self.selected = requested.or(preserved).cloned().or_else(|| {
            let fallback = old_selected_index
                .unwrap_or(0)
                .min(self.visible.len().saturating_sub(1));
            self.id_at_visible(fallback).cloned()
        });
        if self
            .context_menu
            .as_ref()
            .is_some_and(|menu| !self.visible_contains(&menu.id))
        {
            self.context_menu = None;
        }
        self.recalculate_metrics();
        if self.selected != selected_before {
            self.ensure_selection_visible();
        }
    }

    pub fn render(&self, frame: &mut Frame, focused: bool) {
        let title = if self.document.mode == Mode::Flat {
            self.document.panel_action.as_deref().map_or_else(
                || {
                    Line::styled(
                        format!(" {} ", terminal_safe_text(&self.document.title)),
                        Style::default().fg(theme::TEXT),
                    )
                },
                |action| {
                    Line::from(vec![
                        Span::styled(
                            format!(" {} ", terminal_safe_text(&self.document.title)),
                            Style::default().fg(theme::TEXT),
                        ),
                        Span::styled(
                            format!("{} ", terminal_safe_text(action)),
                            enabled_control_style(),
                        ),
                    ])
                },
            )
        } else {
            Line::styled(
                format!(" {} ", terminal_safe_text(&self.document.title)),
                Style::default().fg(theme::TEXT),
            )
        };
        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.document.border_style)
            .title(title);
        if self.document.mode == Mode::Tree && self.area.width >= design::TREE_HEADER_MIN_WIDTH {
            block = block.title(
                Line::styled("[-] [+]", enabled_control_style()).alignment(Alignment::Right),
            );
        }
        frame.render_widget(block, self.area);

        if self.visible.is_empty() {
            frame.render_widget(
                Paragraph::new(terminal_safe_text(&self.document.empty_message)),
                self.metrics.list_area,
            );
            return;
        }

        let selected = focused
            .then(|| self.selected_visible_index())
            .flatten()
            .and_then(|index| {
                index
                    .checked_sub(self.metrics.offset)
                    .filter(|index| *index < usize::from(self.metrics.list_area.height))
            });
        let items = self
            .visible
            .iter()
            .skip(self.metrics.offset)
            .take(usize::from(self.metrics.list_area.height))
            .map(|index| self.list_item(&self.document.rows[*index]));
        let list = List::new(items).highlight_style(
            Style::default()
                .bg(theme::SELECTION_BACKGROUND)
                .add_modifier(Modifier::BOLD),
        );
        let mut state = ListState::default().with_selected(selected);
        frame.render_stateful_widget(list, self.metrics.list_area, &mut state);
        if self.metrics.maximum_offset > 0 && !self.metrics.scrollbar_area.is_empty() {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_style(Style::default().fg(theme::CHROME))
                .thumb_style(Style::default().fg(theme::CHROME));
            let mut state = ScrollbarState::new(scrollbar_position_count(
                self.visible.len(),
                usize::from(self.metrics.list_area.height),
            ))
            .viewport_content_length(usize::from(self.metrics.list_area.height))
            .position(self.metrics.offset);
            frame.render_stateful_widget(scrollbar, self.metrics.scrollbar_area, &mut state);
        }
    }

    pub fn render_menu(&self, frame: &mut Frame) {
        self.render_context_menu(frame);
    }

    pub fn handle_event(&mut self, event: &Event, overlay_area: Rect) -> Option<Outcome<K>> {
        if self.context_menu.is_some() {
            return self.handle_context_menu_event(event, overlay_area);
        }
        if let Event::Mouse(mouse) = event
            && self.area.contains((mouse.column, mouse.row).into())
            && let Some(amount) = wheel_scroll_delta(mouse.kind)
        {
            self.scroll_by(amount);
            return Some(Outcome::Consumed);
        }
        match event {
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::Up(MouseButton::Left) if self.dragging_scrollbar => {
                    self.dragging_scrollbar = false;
                    Some(Outcome::Consumed)
                }
                MouseEventKind::Down(MouseButton::Left)
                    if self
                        .metrics
                        .scrollbar_area
                        .contains((mouse.column, mouse.row).into()) =>
                {
                    self.dragging_scrollbar = true;
                    self.scrollbar_to(mouse.row);
                    Some(Outcome::Consumed)
                }
                MouseEventKind::Drag(MouseButton::Left) if self.dragging_scrollbar => {
                    self.scrollbar_to(mouse.row);
                    Some(Outcome::Consumed)
                }
                MouseEventKind::Down(MouseButton::Right) => {
                    let index = self.row_at(mouse.column, mouse.row)?;
                    let row = &self.document.rows[index];
                    if !row.context_menu {
                        return Some(Outcome::Consumed);
                    }
                    self.selected = Some(row.id.clone());
                    self.context_menu = Some(ContextMenu {
                        id: row.id.clone(),
                        column: mouse.column,
                        row: mouse.row,
                    });
                    Some(Outcome::Selected(row.id.clone()))
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(outcome) = self.header_action_at(mouse.column, mouse.row) {
                        return Some(outcome);
                    }
                    let index = self.row_at(mouse.column, mouse.row)?;
                    let row = &self.document.rows[index];
                    if self.row_action_contains(row, mouse.column) {
                        return Some(Outcome::RowAction(row.id.clone()));
                    }
                    let id = row.id.clone();
                    let branch = row.branch;
                    self.selected = Some(id.clone());
                    if branch {
                        self.toggle_expanded(&id);
                    }
                    self.ensure_selection_visible();
                    Some(Outcome::Selected(id))
                }
                _ => None,
            },
            Event::Key(key) => navigation(key).and_then(|command| self.navigate(command)),
            _ => None,
        }
    }

    pub fn navigate(&mut self, command: Navigation) -> Option<Outcome<K>> {
        let current = self.selected_visible_index().unwrap_or(0);
        match command {
            Navigation::Previous => self.select_visible(current.saturating_sub(1)),
            Navigation::Next => self.select_visible(
                current
                    .saturating_add(1)
                    .min(self.visible.len().saturating_sub(1)),
            ),
            Navigation::First => self.select_visible(0),
            Navigation::Last => self.select_visible(self.visible.len().saturating_sub(1)),
            Navigation::Activate => {
                let id = self.selected.clone()?;
                if self.row(&id).is_some_and(|row| row.branch) {
                    self.toggle_expanded(&id);
                    self.ensure_selection_visible();
                    Some(Outcome::Selected(id))
                } else {
                    Some(Outcome::Activated(id))
                }
            }
            Navigation::OpenMenu => self.open_context_menu(),
        }
    }

    pub fn collapse_all(&mut self) {
        self.expanded.clear();
        self.rebuild_visible();
        self.repair_hidden_selection();
        self.recalculate_metrics();
        self.ensure_selection_visible();
    }

    pub fn expand_all(&mut self) {
        self.expanded = self
            .document
            .rows
            .iter()
            .filter(|row| row.branch)
            .map(|row| row.id.clone())
            .collect();
        self.rebuild_visible();
        self.recalculate_metrics();
        self.ensure_selection_visible();
    }

    #[must_use]
    pub fn selected(&self) -> Option<&K> {
        self.selected.as_ref()
    }

    #[must_use]
    pub const fn metrics(&self) -> Metrics {
        self.metrics
    }

    #[must_use]
    pub fn has_open_menu(&self) -> bool {
        self.context_menu.is_some()
    }

    #[must_use]
    pub fn visible_rows(&self) -> usize {
        self.visible.len()
    }

    fn rebuild_visible(&mut self) {
        self.visible.clear();
        let mut hidden_depth = None;
        for (index, row) in self.document.rows.iter().enumerate() {
            if hidden_depth.is_some_and(|depth| row.depth > depth) {
                continue;
            }
            hidden_depth = None;
            self.visible.push(index);
            if self.document.mode == Mode::Tree && row.branch && !self.expanded.contains(&row.id) {
                hidden_depth = Some(row.depth);
            }
        }
    }

    fn repair_hidden_selection(&mut self) {
        if self
            .selected
            .as_ref()
            .is_some_and(|id| self.visible_contains(id))
        {
            return;
        }
        let ancestor = self.selected.as_ref().and_then(|selected| {
            let selected_index = self
                .document
                .rows
                .iter()
                .position(|row| row.id == *selected)?;
            let selected_depth = self.document.rows[selected_index].depth;
            (0..selected_index).rev().find_map(|index| {
                let row = &self.document.rows[index];
                (row.depth < selected_depth && self.visible.contains(&index))
                    .then(|| row.id.clone())
            })
        });
        self.selected = ancestor.or_else(|| self.id_at_visible(0).cloned());
    }

    fn recalculate_metrics(&mut self) {
        let content = self.area.inner(design::PANEL_INSET);
        let viewport_rows = usize::from(content.height);
        let maximum_offset = maximum_scroll(self.visible.len(), viewport_rows);
        self.offset = self.offset.min(maximum_offset);
        let has_scrollbar = maximum_offset > 0 && content.width > 0;
        self.metrics = Metrics {
            list_area: Rect::new(
                content.x,
                content.y,
                content.width.saturating_sub(u16::from(has_scrollbar)),
                content.height,
            ),
            scrollbar_area: if has_scrollbar {
                Rect::new(
                    content.right().saturating_sub(design::BORDER_WIDTH),
                    content.y,
                    design::BORDER_WIDTH,
                    content.height,
                )
            } else {
                Rect::default()
            },
            maximum_offset,
            offset: self.offset,
        };
    }

    fn ensure_selection_visible(&mut self) {
        let Some(selected) = self.selected_visible_index() else {
            return;
        };
        let rows = usize::from(self.metrics.list_area.height);
        if rows == 0 {
            self.offset = 0;
        } else if selected < self.offset {
            self.offset = selected;
        } else if selected >= self.offset.saturating_add(rows) {
            self.offset = selected.saturating_add(1).saturating_sub(rows);
        }
        self.offset = self.offset.min(self.metrics.maximum_offset);
        self.metrics.offset = self.offset;
    }

    fn list_item(&self, row: &Row<K>) -> ListItem<'static> {
        let mut spans = vec![Span::styled(
            if self.document.mode == Mode::Flat {
                interaction::FLAT_ROW
            } else {
                "  "
            },
            enabled_control_style(),
        )];
        if self.document.mode == Mode::Tree {
            spans.push(Span::raw("  ".repeat(row.depth)));
            spans.push(Span::raw(if row.branch {
                if self.expanded.contains(&row.id) {
                    "▾ "
                } else {
                    "▸ "
                }
            } else {
                "  "
            }));
        }
        spans.extend(row.label.spans.clone());
        if let Some(action) = &row.action {
            let available = usize::from(self.metrics.list_area.width);
            let action_width = Span::raw(action.clone()).width();
            let leading_width = available.saturating_sub(action_width);
            let gap = usize::from(leading_width > 0);
            spans = truncate_spans(&spans, leading_width.saturating_sub(gap));
            let used = Line::from(spans.clone()).width();
            let spacing = available.saturating_sub(used.saturating_add(action_width));
            spans.push(Span::raw(" ".repeat(spacing)));
            spans.push(Span::styled(action.clone(), enabled_control_style()));
        } else {
            spans = truncate_spans(&spans, usize::from(self.metrics.list_area.width));
        }
        ListItem::new(Line::from(spans).style(row.label.style))
    }

    fn select_visible(&mut self, index: usize) -> Option<Outcome<K>> {
        let id = self.id_at_visible(index)?.clone();
        self.selected = Some(id.clone());
        self.ensure_selection_visible();
        Some(Outcome::Selected(id))
    }

    fn toggle_expanded(&mut self, id: &K) {
        if !self.expanded.remove(id) {
            self.expanded.insert(id.clone());
        }
        self.rebuild_visible();
        self.repair_hidden_selection();
        self.recalculate_metrics();
    }

    fn scroll_by(&mut self, amount: i64) {
        self.offset = scroll_offset(self.offset, amount, self.metrics.maximum_offset);
        self.metrics.offset = self.offset;
    }

    fn scrollbar_to(&mut self, row: u16) {
        self.offset = scrollbar_position(
            row.saturating_sub(self.metrics.scrollbar_area.y),
            self.metrics.scrollbar_area.height,
            self.metrics.maximum_offset,
        );
        self.metrics.offset = self.offset;
    }

    fn row_at(&self, column: u16, row: u16) -> Option<usize> {
        self.metrics
            .list_area
            .contains((column, row).into())
            .then(|| {
                self.visible.get(
                    self.metrics
                        .offset
                        .saturating_add(usize::from(row.saturating_sub(self.metrics.list_area.y))),
                )
            })
            .flatten()
            .copied()
    }

    fn row_action_contains(&self, row: &Row<K>, column: u16) -> bool {
        row.action.as_ref().is_some_and(|action| {
            let width = u16::try_from(action.chars().count()).unwrap_or(u16::MAX);
            column >= self.metrics.list_area.right().saturating_sub(width)
        })
    }

    fn header_action_at(&mut self, column: u16, row: u16) -> Option<Outcome<K>> {
        if row != self.area.y {
            return None;
        }
        if self.document.mode == Mode::Tree && self.area.width >= design::TREE_HEADER_MIN_WIDTH {
            let start = self
                .area
                .right()
                .saturating_sub(design::TREE_HEADER_ACTIONS_WIDTH);
            if column >= start && column < start.saturating_add(design::TREE_HEADER_ACTION_WIDTH) {
                self.collapse_all();
                return Some(Outcome::Consumed);
            }
            let expand_start = start
                .saturating_add(design::TREE_HEADER_ACTION_WIDTH)
                .saturating_add(design::TREE_HEADER_ACTION_GAP);
            if column >= expand_start
                && column < expand_start.saturating_add(design::TREE_HEADER_ACTION_WIDTH)
            {
                self.expand_all();
                return Some(Outcome::Consumed);
            }
        } else if let Some(action) = self.document.panel_action.as_deref() {
            let width = u16::try_from(action.chars().count()).unwrap_or(u16::MAX);
            let start = self.area.x.saturating_add(3).saturating_add(
                u16::try_from(self.document.title.chars().count()).unwrap_or(u16::MAX),
            );
            if column >= start && column < start.saturating_add(width) {
                return Some(Outcome::PanelAction);
            }
        }
        None
    }

    fn handle_context_menu_event(
        &mut self,
        event: &Event,
        overlay_area: Rect,
    ) -> Option<Outcome<K>> {
        match event {
            Event::Key(key)
                if key.kind == KeyEventKind::Press
                    && (key.code == KeyCode::Esc
                        || (key.code == KeyCode::Char('c')
                            && key.modifiers == KeyModifiers::NONE)) =>
            {
                self.context_menu = None;
                Some(Outcome::Consumed)
            }
            Event::Key(key)
                if key.kind == KeyEventKind::Press && key.modifiers == KeyModifiers::NONE =>
            {
                let absolute = match key.code {
                    KeyCode::Char('a') => true,
                    KeyCode::Char('r') => false,
                    _ => return None,
                };
                let menu = self.context_menu.take()?;
                Some(Outcome::CopyPath {
                    id: menu.id,
                    absolute,
                })
            }
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                let menu_area = self.context_menu_area(overlay_area)?;
                let menu = self.context_menu.take()?;
                if mouse.column > menu_area.x
                    && mouse.column < menu_area.right().saturating_sub(design::BORDER_WIDTH)
                {
                    match mouse.row.saturating_sub(menu_area.y) {
                        design::PATH_MENU_FIRST_ACTION_ROW => Some(Outcome::CopyPath {
                            id: menu.id,
                            absolute: true,
                        }),
                        design::PATH_MENU_SECOND_ACTION_ROW => Some(Outcome::CopyPath {
                            id: menu.id,
                            absolute: false,
                        }),
                        _ => Some(Outcome::Consumed),
                    }
                } else {
                    Some(Outcome::Consumed)
                }
            }
            _ => None,
        }
    }

    fn open_context_menu(&mut self) -> Option<Outcome<K>> {
        let id = self.selected.clone()?;
        if !self.row(&id).is_some_and(|row| row.context_menu) {
            return None;
        }
        let index = self.selected_visible_index()?;
        let row = self
            .metrics
            .list_area
            .y
            .saturating_add(u16::try_from(index.saturating_sub(self.metrics.offset)).ok()?);
        self.context_menu = Some(ContextMenu {
            id,
            column: self.metrics.list_area.x,
            row,
        });
        Some(Outcome::Consumed)
    }

    fn render_context_menu(&self, frame: &mut Frame) {
        let Some(area) = self.context_menu_area(frame.area()) else {
            return;
        };
        frame.render_widget(Clear, area);
        frame.render_widget(
            List::new([
                ListItem::new("[a] Copy absolute path").style(enabled_control_style()),
                ListItem::new(""),
                ListItem::new("[r] Copy relative path").style(enabled_control_style()),
            ])
            .block(
                Block::default()
                    .title(Line::from(vec![
                        Span::raw(" Path "),
                        Span::styled("[c]", enabled_control_style()),
                        Span::raw(" "),
                    ]))
                    .title(
                        Line::styled(interaction::DISMISS, enabled_control_style())
                            .alignment(Alignment::Right),
                    )
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::CHROME)),
            ),
            area,
        );
    }

    fn context_menu_area(&self, area: Rect) -> Option<Rect> {
        let menu = self.context_menu.as_ref()?;
        let width = design::PATH_MENU_WIDTH.min(area.width);
        let height = design::PATH_MENU_HEIGHT.min(area.height);
        Some(Rect::new(
            menu.column.min(area.right().saturating_sub(width)),
            menu.row.min(area.bottom().saturating_sub(height)),
            width,
            height,
        ))
    }

    fn selected_visible_index(&self) -> Option<usize> {
        let selected = self.selected.as_ref()?;
        self.visible
            .iter()
            .position(|index| self.document.rows[*index].id == *selected)
    }

    fn visible_contains(&self, id: &K) -> bool {
        self.visible
            .iter()
            .any(|index| self.document.rows[*index].id == *id)
    }

    fn id_at_visible(&self, index: usize) -> Option<&K> {
        self.visible
            .get(index)
            .map(|index| &self.document.rows[*index].id)
    }

    fn row(&self, id: &K) -> Option<&Row<K>> {
        self.document.rows.iter().find(|row| &row.id == id)
    }
}

fn truncate_spans(spans: &[Span<'static>], width: usize) -> Vec<Span<'static>> {
    if Line::from(spans.to_vec()).width() <= width {
        return spans.to_vec();
    }
    if width == 0 {
        return Vec::new();
    }

    let ellipsis_width = width.min(3);
    let content_width = width.saturating_sub(ellipsis_width);
    let mut truncated = Vec::new();
    let mut used = 0_usize;
    let mut ellipsis_style = Style::default();

    'spans: for span in spans {
        let mut content = String::new();
        for character in span.content.chars() {
            let character_width = Span::raw(character.to_string()).width();
            if used.saturating_add(character_width) > content_width {
                ellipsis_style = span.style;
                if !content.is_empty() {
                    truncated.push(Span::styled(content, span.style));
                }
                break 'spans;
            }
            content.push(character);
            used = used.saturating_add(character_width);
        }
        if !content.is_empty() {
            truncated.push(Span::styled(content, span.style));
        }
        ellipsis_style = span.style;
    }
    truncated.push(Span::styled(".".repeat(ellipsis_width), ellipsis_style));
    truncated
}

#[must_use]
pub fn navigation(key: &KeyEvent) -> Option<Navigation> {
    if key.kind != KeyEventKind::Press || key.modifiers != KeyModifiers::NONE {
        return None;
    }
    match key.code {
        KeyCode::Char('j' | 'w') => Some(Navigation::Previous),
        KeyCode::Char('k' | 'l' | 's') => Some(Navigation::Next),
        KeyCode::Home | KeyCode::Char('g') => Some(Navigation::First),
        KeyCode::End => Some(Navigation::Last),
        KeyCode::Enter => Some(Navigation::Activate),
        KeyCode::Char('c') => Some(Navigation::OpenMenu),
        _ => None,
    }
}

#[must_use]
pub fn help_rows() -> Vec<(String, &'static str)> {
    vec![
        ("j / w".to_owned(), "Previous file"),
        ("k / l / s".to_owned(), "Next file"),
        ("Home / g".to_owned(), "First file"),
        ("End".to_owned(), "Last file"),
        ("c".to_owned(), "Open path menu"),
    ]
}

#[cfg(test)]
mod tests;
