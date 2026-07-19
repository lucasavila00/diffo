use super::{Message, Model};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::Rect;

use super::{commit_action_at_position, commit_editor_action_at_position};

mod bindings;
mod keyboard;
mod mouse;

pub(crate) fn help_rows() -> Vec<(String, &'static str)> {
    diffo_ui::file_picker::help_rows()
        .into_iter()
        .chain(bindings::help_rows())
        .collect()
}

#[cfg(test)]
use bindings::KEY_BINDINGS;
#[cfg(test)]
use keyboard::map_key;

#[must_use]
pub fn map_event(event: &Event, model: &Model, area: Rect) -> Option<Message> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            keyboard::map_key(key.code, key.modifiers).map(|message| {
                let page_lines = usize::from(
                    area.height
                        .saturating_sub(super::design::DIFF_PAGE_NON_CONTENT_ROWS),
                )
                .max(usize::from(super::design::SINGLE_LINE_HEIGHT));
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

#[must_use]
pub(crate) fn map_commit_event(event: &Event, model: &Model, area: Rect) -> Option<Message> {
    match event {
        Event::Key(key) => keyboard::map_commit_key(key),
        _ => mouse::map_commit_event(event, model, area),
    }
}

#[cfg(test)]
mod input_tests;
