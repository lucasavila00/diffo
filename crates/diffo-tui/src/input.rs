use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_app::{Message, Model};
use ratatui::layout::Rect;

use crate::{
    commit_action_at_position, commit_editor_action_at_position, file_action_at_position,
    file_at_position, file_group_at_position, file_pane_percent_at, is_file_pane_splitter_at,
};

mod bindings;
mod keyboard;
mod mouse;

pub(crate) use bindings::help_rows;

#[cfg(test)]
use bindings::KEY_BINDINGS;
#[cfg(test)]
use keyboard::map_key;

#[must_use]
pub fn map_event(event: &Event, model: &Model, area: Rect) -> Option<Message> {
    if model.help_open {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && matches!(key.code, KeyCode::Char('2') | KeyCode::F(2))
        {
            return Some(Message::ToggleHelp);
        }
        return keyboard::map_help_event(event);
    }
    if model.command_palette.is_some() {
        return keyboard::map_command_palette_event(event);
    }
    if model.commit_input_focused() {
        return match event {
            Event::Key(key) => keyboard::map_commit_key(key),
            _ => mouse::map_commit_event(event, model, area),
        };
    }
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            keyboard::map_key(key.code, key.modifiers).map(|message| {
                let page_lines = usize::from(area.height.saturating_sub(3)).max(1);
                match message {
                    Message::ScrollDiffPageUp(_) => Message::ScrollDiffPageUp(page_lines),
                    Message::ScrollDiffPageDown(_) => Message::ScrollDiffPageDown(page_lines),
                    other => other,
                }
            })
        }
        _ => mouse::map_event(event, model, area),
    }
}

#[cfg(test)]
mod input_tests;
