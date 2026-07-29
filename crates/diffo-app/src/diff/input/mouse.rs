use super::{
    Event, Message, Model, MouseButton, MouseEventKind, Rect, commit_action_at_position,
    commit_editor_action_at_position,
};
use diffo_ui::{
    text_view::{ScrollCommand, wheel_scroll_command},
    wheel_scroll_delta,
};

pub(in crate::diff) fn wheel_message(kind: MouseEventKind) -> Option<Message> {
    match wheel_scroll_command(kind)? {
        ScrollCommand::Lines(lines) => Some(Message::ScrollDiffVerticalBy(lines)),
        ScrollCommand::Columns(columns) => Some(Message::ScrollDiffHorizontalBy(columns)),
        ScrollCommand::Vertical(_)
        | ScrollCommand::Horizontal(_)
        | ScrollCommand::Page(_)
        | ScrollCommand::Home
        | ScrollCommand::End => None,
    }
}

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
        Event::Mouse(mouse)
            if crate::diff::horizontal_panes(
                crate::diff::main_area(area),
                model.file_pane_percent,
            )[1]
            .contains((mouse.column, mouse.row).into()) =>
        {
            wheel_message(mouse.kind)
        }
        Event::Mouse(mouse) => wheel_scroll_delta(mouse.kind).map(Message::ScrollDiffVerticalBy),
        _ => None,
    }
}
