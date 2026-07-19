use super::bindings::KEY_BINDINGS;
use super::{KeyCode, KeyEventKind, KeyModifiers, Message};

pub(super) fn map_key(code: KeyCode, modifiers: KeyModifiers) -> Option<Message> {
    KEY_BINDINGS
        .iter()
        .find(|binding| binding.keys.iter().any(|key| key.matches(code, modifiers)))
        .map(|binding| binding.message.clone())
}

pub(super) fn map_commit_key(key: &crossterm::event::KeyEvent) -> Option<Message> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(Message::Quit);
    }
    match key.code {
        KeyCode::Esc => Some(Message::BlurCommitInput),
        KeyCode::Enter => Some(Message::ExecuteCommit),
        KeyCode::Backspace => Some(Message::CommitMessageBackspace),
        KeyCode::Left => Some(Message::CommitMessageCursorLeft),
        KeyCode::Right => Some(Message::CommitMessageCursorRight),
        KeyCode::Char(character)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            Some(Message::CommitMessageInput(character))
        }
        _ => None,
    }
}
