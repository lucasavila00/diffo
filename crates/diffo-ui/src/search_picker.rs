//! Reusable searchable modal list state, input handling, and rendering.

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::{Clear, List, ListItem, ListState, Paragraph, ScrollbarOrientation},
};

use crate::{
    design, disabled_control_style, enabled_control_style, fuzzy_score, maximum_scroll,
    modal_block, render_scrollbar, terminal_safe_text, theme, wheel_scroll_delta,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchItem<K> {
    pub key: K,
    pub label: String,
    pub aliases: Vec<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchPickerEvent<K> {
    Consumed,
    Cancel,
    Activate(K),
    Quit,
}

pub struct SearchPicker<K> {
    title: &'static str,
    empty_message: &'static str,
    query: String,
    items: Vec<SearchItem<K>>,
    selected: Option<usize>,
    offset: usize,
}

impl<K> SearchPicker<K>
where
    K: Clone,
{
    #[must_use]
    pub fn new(title: &'static str, empty_message: &'static str) -> Self {
        Self {
            title,
            empty_message,
            query: String::new(),
            items: Vec::new(),
            selected: None,
            offset: 0,
        }
    }

    pub fn set_empty_message(&mut self, message: &'static str) {
        self.empty_message = message;
    }

    pub fn set_items(&mut self, items: Vec<SearchItem<K>>) {
        self.items = items;
        self.offset = 0;
        self.select_first_enabled();
    }

    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    #[must_use]
    pub fn selected_key(&self) -> Option<&K> {
        let matches = self.matches();
        self.selected
            .and_then(|selected| matches.get(selected))
            .map(|(_, item)| &item.key)
    }

    pub fn handle_event(&mut self, event: &Event, area: Rect) -> SearchPickerEvent<K> {
        let (_, results) = search_picker_layout(area);
        if let Event::Mouse(mouse) = event {
            if let Some(amount) = wheel_scroll_delta(mouse.kind)
                && results.contains((mouse.column, mouse.row).into())
            {
                self.scroll(amount, usize::from(results.height));
                return SearchPickerEvent::Consumed;
            }
            if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                && results.contains((mouse.column, mouse.row).into())
            {
                let visible = usize::from(mouse.row.saturating_sub(results.y));
                let index = self.offset.saturating_add(visible);
                let matches = self.matches();
                let activated = matches
                    .get(index)
                    .filter(|(_, item)| item.enabled)
                    .map(|(_, item)| item.key.clone());
                if let Some(key) = activated {
                    self.selected = Some(index);
                    return SearchPickerEvent::Activate(key);
                }
            }
            return SearchPickerEvent::Consumed;
        }
        let Event::Key(key) = event else {
            return SearchPickerEvent::Consumed;
        };
        if key.kind != KeyEventKind::Press {
            return SearchPickerEvent::Consumed;
        }
        match key.code {
            KeyCode::Esc => SearchPickerEvent::Cancel,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                SearchPickerEvent::Quit
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.offset = 0;
                self.select_first_enabled();
                SearchPickerEvent::Consumed
            }
            KeyCode::Up => {
                self.move_selection(-1);
                self.ensure_selection_visible(usize::from(results.height));
                SearchPickerEvent::Consumed
            }
            KeyCode::Down => {
                self.move_selection(1);
                self.ensure_selection_visible(usize::from(results.height));
                SearchPickerEvent::Consumed
            }
            KeyCode::Enter => self
                .selected_key()
                .cloned()
                .map_or(SearchPickerEvent::Consumed, SearchPickerEvent::Activate),
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.query.push(character);
                self.offset = 0;
                self.select_first_enabled();
                SearchPickerEvent::Consumed
            }
            _ => SearchPickerEvent::Consumed,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let matches = self.matches();
        let (modal, results) = search_picker_layout(area);
        frame.render_widget(Clear, modal);
        frame.render_widget(modal_block(self.title), modal);
        let inner = modal.inner(design::DIALOG_INSET);
        let sections = search_picker_sections(inner);
        frame.render_widget(
            Paragraph::new(format!("> {}█", terminal_safe_text(&self.query)))
                .style(enabled_control_style()),
            sections[0],
        );
        frame.render_widget(
            Paragraph::new("─".repeat(usize::from(sections[1].width)))
                .style(Style::default().fg(theme::CHROME)),
            sections[1],
        );
        let viewport = usize::from(results.height);
        let items = if matches.is_empty() {
            vec![ListItem::new(self.empty_message).style(disabled_control_style())]
        } else {
            matches
                .iter()
                .skip(self.offset)
                .take(viewport)
                .map(|(_, item)| {
                    ListItem::new(terminal_safe_text(&item.label)).style(if item.enabled {
                        enabled_control_style()
                    } else {
                        disabled_control_style()
                    })
                })
                .collect()
        };
        let selected = self.selected.and_then(|selected| {
            selected
                .checked_sub(self.offset)
                .filter(|selected| *selected < viewport)
        });
        let list = List::new(items)
            .highlight_symbol("› ")
            .highlight_style(enabled_control_style().bg(theme::SELECTION_BACKGROUND));
        let mut state = ListState::default().with_selected(selected);
        frame.render_stateful_widget(list, results, &mut state);
        let maximum = maximum_scroll(matches.len(), viewport);
        if maximum > 0 && results.width > 0 {
            let scrollbar = Rect::new(
                results.right().saturating_sub(1),
                results.y,
                1,
                results.height,
            );
            render_scrollbar(
                frame,
                scrollbar,
                &ScrollbarOrientation::VerticalRight,
                matches.len(),
                viewport,
                self.offset,
                Style::default().fg(theme::CHROME),
            );
        }
        frame.render_widget(
            Paragraph::new(Line::styled(
                "↑/↓ select · Enter choose · Esc close",
                enabled_control_style(),
            )),
            sections[3],
        );
    }

    fn matches(&self) -> Vec<(usize, &SearchItem<K>)> {
        let mut matches = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let score = std::iter::once(item.label.as_str())
                    .chain(item.aliases.iter().map(String::as_str))
                    .filter_map(|candidate| fuzzy_score(candidate, &self.query))
                    .max()?;
                Some((score, index, item))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        matches
            .into_iter()
            .map(|(_, index, item)| (index, item))
            .collect()
    }

    fn select_first_enabled(&mut self) {
        self.selected = self.matches().iter().position(|(_, item)| item.enabled);
    }

    fn move_selection(&mut self, amount: i64) {
        let enabled = self
            .matches()
            .iter()
            .enumerate()
            .filter_map(|(index, (_, item))| item.enabled.then_some(index))
            .collect::<Vec<_>>();
        let Some(current) = self.selected else {
            self.selected = enabled.first().copied();
            return;
        };
        let position = enabled
            .iter()
            .position(|index| *index == current)
            .unwrap_or(0);
        let next = if amount < 0 {
            position.saturating_sub(1)
        } else {
            position
                .saturating_add(1)
                .min(enabled.len().saturating_sub(1))
        };
        self.selected = enabled.get(next).copied();
    }

    fn scroll(&mut self, amount: i64, viewport: usize) {
        let maximum = maximum_scroll(self.matches().len(), viewport);
        self.offset = crate::scroll_offset(self.offset, amount, maximum);
    }

    fn ensure_selection_visible(&mut self, viewport: usize) {
        let Some(selected) = self.selected else {
            return;
        };
        if selected < self.offset {
            self.offset = selected;
        } else if selected >= self.offset.saturating_add(viewport) {
            self.offset = selected.saturating_add(1).saturating_sub(viewport);
        }
    }
}

#[must_use]
pub fn search_picker_layout(area: Rect) -> (Rect, Rect) {
    let width = design::SEARCH_PICKER_WIDTH.resolve(area.width);
    let top = area.y.saturating_add(
        area.height
            .saturating_mul(design::SEARCH_PICKER_TOP_PERCENT)
            / design::FULL_PERCENT,
    );
    let height = design::SEARCH_PICKER_MAX_HEIGHT.min(area.bottom().saturating_sub(top));
    let modal = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        top,
        width,
        height,
    );
    let inner = modal.inner(design::DIALOG_INSET);
    (modal, search_picker_sections(inner)[2])
}

fn search_picker_sections(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::vertical([
        Constraint::Length(design::SINGLE_LINE_HEIGHT),
        Constraint::Length(design::SINGLE_LINE_HEIGHT),
        Constraint::Min(design::SINGLE_LINE_HEIGHT),
        Constraint::Length(design::SINGLE_LINE_HEIGHT),
    ])
    .split(area)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, MouseEvent};

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn aliases_rank_stably_and_disabled_rows_cannot_activate() {
        let mut picker = SearchPicker::new("Branches", "None");
        picker.set_items(vec![
            SearchItem {
                key: 1,
                label: "main".to_owned(),
                aliases: Vec::new(),
                enabled: false,
            },
            SearchItem {
                key: 2,
                label: "topic".to_owned(),
                aliases: Vec::new(),
                enabled: true,
            },
            SearchItem {
                key: 3,
                label: "origin/topic".to_owned(),
                aliases: vec!["topic".to_owned()],
                enabled: true,
            },
        ]);

        assert_eq!(picker.selected_key(), Some(&2));
        let _ = picker.handle_event(&key(KeyCode::Char('T')), Rect::new(0, 0, 80, 24));
        assert_eq!(picker.query(), "T");
        assert_eq!(picker.selected_key(), Some(&2));
        assert_eq!(
            picker.handle_event(&key(KeyCode::Enter), Rect::new(0, 0, 80, 24)),
            SearchPickerEvent::Activate(2)
        );
    }

    #[test]
    fn escape_closes_and_pointer_movement_changes_nothing() {
        let mut picker = SearchPicker::new("Branches", "None");
        picker.set_items(vec![SearchItem {
            key: 1,
            label: "main".to_owned(),
            aliases: Vec::new(),
            enabled: true,
        }]);
        let moved = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column: 10,
            row: 10,
            modifiers: KeyModifiers::NONE,
        });

        assert_eq!(
            picker.handle_event(&moved, Rect::new(0, 0, 80, 24)),
            SearchPickerEvent::Consumed
        );
        assert_eq!(picker.selected_key(), Some(&1));
        assert_eq!(
            picker.handle_event(&key(KeyCode::Esc), Rect::new(0, 0, 80, 24)),
            SearchPickerEvent::Cancel
        );
    }
}
