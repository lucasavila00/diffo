use super::*;

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
            if let Some(message) = commit_action_at_position(model, area, mouse.column, mouse.row) {
                Some(message)
            } else if is_file_pane_splitter_at(model, area, mouse.column, mouse.row) {
                Some(Message::BeginFilePaneResize)
            } else if let Some(message) =
                file_action_at_position(model, area, mouse.column, mouse.row)
            {
                Some(message)
            } else {
                file_at_position(model, area, mouse.column, mouse.row).map(Message::SelectFile)
            }
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Right) => {
            file_at_position(model, area, mouse.column, mouse.row)
                .map(|file| Message::OpenFileContextMenu(file, mouse.column, mouse.row))
        }
        Event::Mouse(mouse)
            if mouse.kind == MouseEventKind::Drag(MouseButton::Left)
                && model.resizing_file_pane =>
        {
            Some(Message::ResizeFilePane(file_pane_percent_at(
                area,
                mouse.column,
            )))
        }
        Event::Mouse(mouse)
            if mouse.kind == MouseEventKind::Up(MouseButton::Left) && model.resizing_file_pane =>
        {
            Some(Message::EndFilePaneResize)
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::ScrollUp => {
            Some(Message::ScrollDiffBy(-1))
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::ScrollDown => {
            Some(Message::ScrollDiffBy(1))
        }
        _ => None,
    }
}
