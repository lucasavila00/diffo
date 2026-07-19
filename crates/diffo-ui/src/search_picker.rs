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
pub struct SearchItem<I, P> {
    pub identity: I,
    pub payload: P,
    pub label: String,
    pub aliases: Vec<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchPickerEvent<P> {
    Consumed,
    Cancel,
    Activate(P),
    Quit,
}

pub struct SearchPicker<I, P> {
    title: &'static str,
    empty_message: &'static str,
    query: String,
    items: Vec<SearchItem<I, P>>,
    selected: Option<usize>,
    offset: usize,
}

impl<I, P> SearchPicker<I, P>
where
    I: Clone + Eq,
    P: Clone,
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

    pub fn set_items(&mut self, items: Vec<SearchItem<I, P>>) {
        self.items = items;
        self.offset = 0;
        self.select_first_enabled();
    }

    pub fn reconcile_items(&mut self, items: Vec<SearchItem<I, P>>) {
        let matches = self.matches();
        let selected_identity = self
            .selected
            .and_then(|selected| matches.get(selected))
            .map(|(_, item)| item.identity.clone());
        let top_identity = matches
            .get(self.offset)
            .map(|(_, item)| item.identity.clone());
        let old_offset = self.offset;

        self.items = items;
        let (selected, offset) = {
            let matches = self.matches();
            let preserved_selection = selected_identity.as_ref().and_then(|identity| {
                matches
                    .iter()
                    .position(|(_, item)| item.enabled && item.identity == *identity)
            });
            let selected =
                preserved_selection.or_else(|| matches.iter().position(|(_, item)| item.enabled));
            let offset = if preserved_selection.is_some() {
                top_identity
                    .as_ref()
                    .and_then(|identity| {
                        matches
                            .iter()
                            .position(|(_, item)| item.identity == *identity)
                    })
                    .unwrap_or(old_offset)
                    .min(matches.len().saturating_sub(1))
            } else {
                0
            };
            (selected, offset)
        };
        self.selected = selected;
        self.offset = offset;
    }

    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    #[must_use]
    pub fn selected_identity(&self) -> Option<&I> {
        let matches = self.matches();
        self.selected
            .and_then(|selected| matches.get(selected))
            .map(|(_, item)| &item.identity)
    }

    pub fn handle_event(&mut self, event: &Event, area: Rect) -> SearchPickerEvent<P> {
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
                    .map(|(_, item)| item.payload.clone());
                if let Some(payload) = activated {
                    self.selected = Some(index);
                    return SearchPickerEvent::Activate(payload);
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
                .selected
                .and_then(|selected| {
                    self.matches()
                        .get(selected)
                        .map(|(_, item)| item.payload.clone())
                })
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

    fn matches(&self) -> Vec<(usize, &SearchItem<I, P>)> {
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
                identity: 1,
                payload: "main-a",
                label: "main".to_owned(),
                aliases: Vec::new(),
                enabled: false,
            },
            SearchItem {
                identity: 2,
                payload: "topic-a",
                label: "topic".to_owned(),
                aliases: Vec::new(),
                enabled: true,
            },
            SearchItem {
                identity: 3,
                payload: "remote-topic-a",
                label: "origin/topic".to_owned(),
                aliases: vec!["topic".to_owned()],
                enabled: true,
            },
        ]);

        assert_eq!(picker.selected_identity(), Some(&2));
        let _ = picker.handle_event(&key(KeyCode::Char('T')), Rect::new(0, 0, 80, 24));
        assert_eq!(picker.query(), "T");
        assert_eq!(picker.selected_identity(), Some(&2));
        assert_eq!(
            picker.handle_event(&key(KeyCode::Enter), Rect::new(0, 0, 80, 24)),
            SearchPickerEvent::Activate("topic-a")
        );
    }

    #[test]
    fn escape_closes_and_pointer_movement_changes_nothing() {
        let mut picker = SearchPicker::new("Branches", "None");
        picker.set_items(vec![SearchItem {
            identity: 1,
            payload: "main-a",
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
        assert_eq!(picker.selected_identity(), Some(&1));
        assert_eq!(
            picker.handle_event(&key(KeyCode::Esc), Rect::new(0, 0, 80, 24)),
            SearchPickerEvent::Cancel
        );
    }

    #[test]
    fn refresh_preserves_interaction_by_identity_and_uses_the_newest_payload() {
        let area = Rect::new(0, 0, 80, 24);
        let item = |identity, payload: &'static str, label: &str, enabled| SearchItem {
            identity,
            payload,
            label: label.to_owned(),
            aliases: Vec::new(),
            enabled,
        };
        let mut picker = SearchPicker::new("Branches", "None");
        picker.set_items(vec![
            item(1, "main-a", "main", true),
            item(2, "topic-a", "topic", true),
            item(3, "other-a", "other", true),
        ]);
        let _ = picker.handle_event(&key(KeyCode::Down), area);
        let _ = picker.handle_event(&key(KeyCode::Char('t')), area);
        assert_eq!(picker.selected_identity(), Some(&2));

        picker.reconcile_items(vec![
            item(3, "other-b", "other", true),
            item(2, "topic-b", "topic", true),
            item(1, "main-b", "main", true),
        ]);

        assert_eq!(picker.query(), "t");
        assert_eq!(picker.selected_identity(), Some(&2));
        assert_eq!(
            picker.handle_event(&key(KeyCode::Enter), area),
            SearchPickerEvent::Activate("topic-b")
        );
    }

    #[test]
    fn refresh_reconciles_removed_disabled_filtered_and_empty_selections() {
        let area = Rect::new(0, 0, 80, 24);
        let item = |identity, label: &str, enabled| SearchItem {
            identity,
            payload: identity,
            label: label.to_owned(),
            aliases: Vec::new(),
            enabled,
        };
        let mut picker = SearchPicker::new("Branches", "None");
        picker.set_items(vec![item(1, "main", true), item(2, "topic", true)]);
        let _ = picker.handle_event(&key(KeyCode::Down), area);
        picker.offset = 1;

        picker.reconcile_items(vec![item(1, "main", true)]);
        assert_eq!(picker.selected_identity(), Some(&1));
        assert_eq!(picker.offset, 0);
        picker.reconcile_items(vec![item(1, "main", false), item(2, "topic", true)]);
        assert_eq!(picker.selected_identity(), Some(&2));
        let _ = picker.handle_event(&key(KeyCode::Char('m')), area);
        assert_eq!(picker.selected_identity(), None);
        picker.reconcile_items(Vec::new());
        assert_eq!(picker.query(), "m");
        assert_eq!(picker.selected_identity(), None);
    }

    #[test]
    fn refresh_keeps_the_scrolled_top_item_by_identity() {
        let item = |identity| SearchItem {
            identity,
            payload: identity,
            label: format!("branch-{identity:02}"),
            aliases: Vec::new(),
            enabled: true,
        };
        let mut picker = SearchPicker::new("Branches", "None");
        picker.set_items((0..20).map(item).collect());
        picker.offset = 7;

        picker.reconcile_items((0..20).rev().map(item).collect());

        assert_eq!(picker.offset, 12);
        assert_eq!(picker.matches()[picker.offset].1.identity, 7);

        picker.reconcile_items(
            (0..20)
                .rev()
                .filter(|identity| *identity != 7)
                .map(item)
                .collect(),
        );
        assert_eq!(picker.offset, 12);
    }
}
