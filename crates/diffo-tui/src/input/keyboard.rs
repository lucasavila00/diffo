use super::bindings::KEY_BINDINGS;
use super::*;

pub(super) fn map_help_event(event: &Event) -> Option<Message> {
    let Event::Key(key) = event else {
        return None;
    };
    if key.kind != KeyEventKind::Press {
        return None;
    }
    match key.code {
        KeyCode::Esc => Some(Message::CloseHelp),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Message::Quit),
        _ => None,
    }
}

pub(super) fn map_command_palette_event(event: &Event) -> Option<Message> {
    let Event::Key(key) = event else {
        return None;
    };
    if key.kind != KeyEventKind::Press {
        return None;
    }
    match key.code {
        KeyCode::Esc => Some(Message::CloseCommandPalette),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Message::Quit),
        KeyCode::Backspace => Some(Message::CommandPaletteBackspace),
        KeyCode::Up => Some(Message::CommandPaletteSelectPrevious),
        KeyCode::Down => Some(Message::CommandPaletteSelectNext),
        KeyCode::Enter => Some(Message::ExecuteSelectedCommand),
        KeyCode::Char(character)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            Some(Message::CommandPaletteInput(character))
        }
        _ => None,
    }
}

pub(super) fn map_key(
    code: KeyCode,
    modifiers: KeyModifiers,
    access_mode: AccessMode,
) -> Option<Message> {
    KEY_BINDINGS
        .iter()
        .filter(|binding| binding.is_available(access_mode))
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
        KeyCode::Enter => Some(Message::ExecutePrimaryAction),
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
