use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};

use crate::UiAction;

pub(crate) const READ_ONLY_HELP: &str =
    " j: previous  k: next  arrows: scroll diff  q: quit  read-only ";
pub(crate) const READ_WRITE_HELP: &str =
    " j: previous  k: next  arrows: scroll diff  s: stage  u: unstage  a: stage all  q: quit ";

#[must_use]
pub fn map_event(event: &Event) -> Option<UiAction> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Some(UiAction::Quit),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(UiAction::Quit)
            }
            KeyCode::Char('j') => Some(UiAction::SelectPreviousFile),
            KeyCode::Char('k') => Some(UiAction::SelectNextFile),
            KeyCode::Up => Some(UiAction::ScrollDiffUp),
            KeyCode::Down => Some(UiAction::ScrollDiffDown),
            KeyCode::Left => Some(UiAction::ScrollDiffLeft),
            KeyCode::Right => Some(UiAction::ScrollDiffRight),
            KeyCode::Home | KeyCode::Char('g') => Some(UiAction::SelectFirstFile),
            KeyCode::End | KeyCode::Char('G') => Some(UiAction::SelectLastFile),
            KeyCode::Char('s') => Some(UiAction::StageSelected),
            KeyCode::Char('u') => Some(UiAction::UnstageSelected),
            KeyCode::Char('a') => Some(UiAction::StageAll),
            _ => None,
        },
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
            Some(UiAction::SelectAt {
                column: mouse.column,
                row: mouse.row,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton,
        MouseEvent, MouseEventKind,
    };

    use super::map_event;
    use crate::UiAction;

    #[test]
    fn maps_fixed_key_bindings() {
        let cases = [
            (KeyCode::Char('q'), UiAction::Quit),
            (KeyCode::Esc, UiAction::Quit),
            (KeyCode::Char('j'), UiAction::SelectPreviousFile),
            (KeyCode::Char('k'), UiAction::SelectNextFile),
            (KeyCode::Up, UiAction::ScrollDiffUp),
            (KeyCode::Down, UiAction::ScrollDiffDown),
            (KeyCode::Left, UiAction::ScrollDiffLeft),
            (KeyCode::Right, UiAction::ScrollDiffRight),
            (KeyCode::Home, UiAction::SelectFirstFile),
            (KeyCode::End, UiAction::SelectLastFile),
            (KeyCode::Char('s'), UiAction::StageSelected),
            (KeyCode::Char('u'), UiAction::UnstageSelected),
            (KeyCode::Char('a'), UiAction::StageAll),
        ];

        for (key, expected) in cases {
            assert_eq!(
                map_event(&Event::Key(KeyEvent::new(key, KeyModifiers::NONE))),
                Some(expected)
            );
        }
    }

    #[test]
    fn maps_control_c_and_left_click() {
        assert_eq!(
            map_event(&Event::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
            ))),
            Some(UiAction::Quit)
        );
        assert_eq!(
            map_event(&Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 12,
                row: 8,
                modifiers: KeyModifiers::NONE,
            })),
            Some(UiAction::SelectAt { column: 12, row: 8 })
        );
    }

    #[test]
    fn ignores_repeat_release_and_unknown_events() {
        for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
            let key = KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::NONE,
                kind,
                state: KeyEventState::NONE,
            };
            assert_eq!(map_event(&Event::Key(key)), None);
        }
        assert_eq!(
            map_event(&Event::Key(KeyEvent::new(
                KeyCode::Char('x'),
                KeyModifiers::NONE,
            ))),
            None
        );
    }
}
