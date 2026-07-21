use std::hash::Hash;

use crate::{design, icons, mouse_target_style, terminal_safe_text, theme};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem},
};

use super::{ContextMenu, FilePicker, Mode, Outcome};

impl<K> FilePicker<K>
where
    K: Clone + Eq + Hash,
{
    pub(super) fn header_action_at(&mut self, column: u16, row: u16) -> Option<Outcome<K>> {
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

    pub(super) fn handle_context_menu_event(
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
                        design::PATH_MENU_THIRD_ACTION_ROW
                            if self
                                .row(&menu.id)
                                .is_some_and(|row| row.destructive_action.is_some()) =>
                        {
                            Some(Outcome::DestructiveAction(menu.id))
                        }
                        _ => Some(Outcome::Consumed),
                    }
                } else {
                    Some(Outcome::Consumed)
                }
            }
            _ => None,
        }
    }

    pub(super) fn open_context_menu(&mut self) -> Option<Outcome<K>> {
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

    pub(super) fn render_context_menu(&self, frame: &mut Frame) {
        let Some(area) = self.context_menu_area(frame.area()) else {
            return;
        };
        frame.render_widget(Clear, area);
        let mut items = vec![
            ListItem::new("[a] Copy absolute path").style(mouse_target_style()),
            ListItem::new(""),
            ListItem::new("[r] Copy relative path").style(mouse_target_style()),
        ];
        if let Some(action) = self
            .context_menu
            .as_ref()
            .and_then(|menu| self.row(&menu.id))
            .and_then(|row| row.destructive_action.as_deref())
        {
            items.push(ListItem::new(""));
            items.push(
                ListItem::new(terminal_safe_text(action)).style(Style::default().fg(theme::DANGER)),
            );
        }
        frame.render_widget(
            List::new(items).block(
                Block::default()
                    .title(Line::from(vec![
                        Span::raw(" Path "),
                        Span::styled("[c]", mouse_target_style()),
                        Span::raw(" "),
                    ]))
                    .title(
                        Line::styled(icons::DISMISS, mouse_target_style())
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
        let height = if self
            .row(&menu.id)
            .is_some_and(|row| row.destructive_action.is_some())
        {
            design::PATH_MENU_DESTRUCTIVE_HEIGHT
        } else {
            design::PATH_MENU_HEIGHT
        }
        .min(area.height);
        Some(Rect::new(
            menu.column.min(area.right().saturating_sub(width)),
            menu.row.min(area.bottom().saturating_sub(height)),
            width,
            height,
        ))
    }
}
