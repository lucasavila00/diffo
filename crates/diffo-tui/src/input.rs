use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_app::{Message, Model};
use ratatui::layout::Rect;

use crate::file_at_position;

pub(crate) const READ_ONLY_HELP: &str =
    " j: previous  k: next  arrows: scroll diff  q: quit  read-only ";
pub(crate) const READ_WRITE_HELP: &str =
    " j: previous  k: next  arrows: scroll diff  s: stage  u: unstage  a: stage all  q: quit ";

#[must_use]
pub fn map_event(event: &Event, model: &Model, area: Rect) -> Option<Message> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => map_key(key.code, key.modifiers),
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
            file_at_position(model, area, mouse.column, mouse.row).map(Message::SelectFile)
        }
        _ => None,
    }
}

fn map_key(code: KeyCode, modifiers: KeyModifiers) -> Option<Message> {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => Some(Message::Quit),
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => Some(Message::Quit),
        KeyCode::Char('j') => Some(Message::SelectPreviousFile),
        KeyCode::Char('k') => Some(Message::SelectNextFile),
        KeyCode::Up => Some(Message::ScrollDiffUp),
        KeyCode::Down => Some(Message::ScrollDiffDown),
        KeyCode::Left => Some(Message::ScrollDiffLeft),
        KeyCode::Right => Some(Message::ScrollDiffRight),
        KeyCode::Home | KeyCode::Char('g') => Some(Message::SelectFirstFile),
        KeyCode::End | KeyCode::Char('G') => Some(Message::SelectLastFile),
        KeyCode::Char('s') => Some(Message::StageSelected),
        KeyCode::Char('u') => Some(Message::UnstageSelected),
        KeyCode::Char('a') => Some(Message::StageAll),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton,
        MouseEvent, MouseEventKind,
    };
    use diffo_app::{ChangeArea, FileKey, Message, Model};
    use diffo_core::{AccessMode, ChangeKind, FileState, RepositorySnapshot};
    use ratatui::layout::Rect;

    use super::map_event;

    fn model() -> Model {
        Model::new(
            RepositorySnapshot {
                files: vec![FileState {
                    path: PathBuf::from("file.txt"),
                    old_path: None,
                    kind: ChangeKind::Untracked,
                    staged: None,
                    unstaged: None,
                }],
                ..RepositorySnapshot::default()
            },
            AccessMode::ReadWrite,
        )
    }

    #[test]
    fn maps_fixed_key_bindings() {
        let cases = [
            (KeyCode::Char('q'), Message::Quit),
            (KeyCode::Esc, Message::Quit),
            (KeyCode::Char('j'), Message::SelectPreviousFile),
            (KeyCode::Char('k'), Message::SelectNextFile),
            (KeyCode::Up, Message::ScrollDiffUp),
            (KeyCode::Down, Message::ScrollDiffDown),
            (KeyCode::Left, Message::ScrollDiffLeft),
            (KeyCode::Right, Message::ScrollDiffRight),
            (KeyCode::Home, Message::SelectFirstFile),
            (KeyCode::End, Message::SelectLastFile),
            (KeyCode::Char('s'), Message::StageSelected),
            (KeyCode::Char('u'), Message::UnstageSelected),
            (KeyCode::Char('a'), Message::StageAll),
        ];
        let model = model();
        for (key, expected) in cases {
            assert_eq!(
                map_event(
                    &Event::Key(KeyEvent::new(key, KeyModifiers::NONE)),
                    &model,
                    Rect::default(),
                ),
                Some(expected)
            );
        }
    }

    #[test]
    fn maps_control_c_and_file_click() {
        let model = model();
        assert_eq!(
            map_event(
                &Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL,)),
                &model,
                Rect::default(),
            ),
            Some(Message::Quit)
        );
        assert_eq!(
            map_event(
                &Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: 4,
                    row: 2,
                    modifiers: KeyModifiers::NONE,
                }),
                &model,
                Rect::new(0, 0, 100, 30),
            ),
            Some(Message::SelectFile(FileKey {
                path: PathBuf::from("file.txt"),
                area: ChangeArea::Unstaged,
            }))
        );
    }

    #[test]
    fn ignores_non_press_unknown_and_non_file_clicks() {
        let model = model();
        for kind in [KeyEventKind::Repeat, KeyEventKind::Release] {
            let key = KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::NONE,
                kind,
                state: KeyEventState::NONE,
            };
            assert_eq!(map_event(&Event::Key(key), &model, Rect::default()), None);
        }
        assert_eq!(
            map_event(
                &Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)),
                &model,
                Rect::default(),
            ),
            None
        );
        assert_eq!(
            map_event(
                &Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: 4,
                    row: 1,
                    modifiers: KeyModifiers::NONE,
                }),
                &model,
                Rect::new(0, 0, 100, 30),
            ),
            None
        );
    }
}
