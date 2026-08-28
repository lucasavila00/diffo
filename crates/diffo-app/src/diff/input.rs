use super::{Message, Model};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_ui::file_picker::Outcome as PickerOutcome;
use ratatui::layout::Rect;

use super::{
    ChangeArea, FileKey, RendererEvent, commit_action_at_position, commit_editor_action_at_position,
};

mod bindings;
mod keyboard;
mod mouse;

pub(crate) fn help_rows() -> Vec<(String, &'static str)> {
    diffo_ui::file_picker::help_rows()
        .into_iter()
        .chain(bindings::help_rows())
        .collect()
}

pub(crate) use bindings::review_help_rows;

#[cfg(test)]
use bindings::KEY_BINDINGS;
#[cfg(test)]
use keyboard::map_key;

#[must_use]
pub(in crate::diff) fn map_review_event(event: &Event, area: Rect) -> Option<Message> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            let message = keyboard::map_key(key.code, key.modifiers)?;
            is_review_message(&message).then_some(message)
        }
        Event::Mouse(mouse) if area.contains((mouse.column, mouse.row).into()) => {
            mouse::wheel_message(mouse.kind)
        }
        _ => None,
    }
}

fn is_review_message(message: &Message) -> bool {
    matches!(
        message,
        Message::ScrollDiffUp
            | Message::ScrollDiffDown
            | Message::ScrollDiffPageUp(_)
            | Message::ScrollDiffPageDown(_)
            | Message::ScrollDiffVerticalBy(_)
            | Message::SetDiffScroll(_)
            | Message::SetDiffHorizontalScroll(_)
            | Message::ScrollDiffLeft
            | Message::ScrollDiffRight
            | Message::ScrollDiffHorizontalBy(_)
            | Message::JumpToPreviousChange
            | Message::JumpToNextChange
            | Message::ToggleDiffView
    )
}

pub(super) fn picker_event(outcome: PickerOutcome<FileKey>, area: ChangeArea) -> RendererEvent {
    match outcome {
        PickerOutcome::Consumed => RendererEvent::Consumed,
        PickerOutcome::Selected(file) | PickerOutcome::Activated(file) => {
            RendererEvent::Message(Message::SelectFile(file))
        }
        PickerOutcome::RowAction(file) => RendererEvent::Message(match file.area {
            ChangeArea::Staged => Message::UnstageFile(file.path),
            ChangeArea::Unstaged => Message::StageFile(file.path),
        }),
        PickerOutcome::PanelAction => RendererEvent::Message(match area {
            ChangeArea::Staged => Message::UnstageAll,
            ChangeArea::Unstaged => Message::StageAll,
        }),
        PickerOutcome::CopyPath { id, absolute } => RendererEvent::CopyPath {
            path: id.path,
            absolute,
        },
        PickerOutcome::DestructiveAction(file) => {
            RendererEvent::Message(Message::RequestDiscardFile(file.path))
        }
    }
}

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
