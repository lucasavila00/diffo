use super::{
    Event, Message, Model, MouseButton, MouseEventKind, Rect, commit_action_at_position,
    commit_editor_action_at_position,
};
use diffo_text_view::WHEEL_SCROLL_ROWS;

pub(super) fn map_commit_event(event: &Event, model: &Model, area: Rect) -> Option<Message> {
    match event {
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
            commit_editor_action_at_position(model, area, mouse.column, mouse.row)
        }
        _ => None,
    }
}

pub(super) fn map_event(event: &Event, model: &Model, area: Rect) -> Option<Message> {
    match event {
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
            commit_action_at_position(model, area, mouse.column, mouse.row)
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::ScrollUp => {
            Some(Message::ScrollDiffVerticalBy(-WHEEL_SCROLL_ROWS))
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::ScrollDown => {
            Some(Message::ScrollDiffVerticalBy(WHEEL_SCROLL_ROWS))
        }
        _ => None,
    }
}
