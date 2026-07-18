use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use diffo_app::{Message, Model};
use diffo_core::{ChangeKind, FileDiff, FileState, RepositorySnapshot};
use ratatui::layout::Rect;

use super::{KEY_BINDINGS, help_rows, map_event, map_key};

fn model() -> Model {
    Model::new(RepositorySnapshot {
        files: vec![FileState {
            path: PathBuf::from("file.txt"),
            old_path: None,
            kind: ChangeKind::Untracked,
            staged: None,
            unstaged: None,
        }],
        ..RepositorySnapshot::default()
    })
}

#[test]
fn maps_fixed_key_bindings() {
    let cases = [
        (KeyCode::Char('q'), Message::Quit),
        (KeyCode::Char('2'), Message::ToggleHelp),
        (KeyCode::F(2), Message::ToggleHelp),
        (KeyCode::Esc, Message::Quit),
        (KeyCode::Up, Message::ScrollDiffVerticalBy(-4)),
        (KeyCode::Down, Message::ScrollDiffVerticalBy(4)),
        (KeyCode::PageUp, Message::ScrollDiffPageUp(1)),
        (KeyCode::PageDown, Message::ScrollDiffPageDown(1)),
        (KeyCode::Left, Message::ScrollDiffHorizontalBy(-4)),
        (KeyCode::Right, Message::ScrollDiffHorizontalBy(4)),
        (KeyCode::Char('r'), Message::ToggleDiffView),
        (KeyCode::Char('n'), Message::JumpToNextChange),
        (KeyCode::Char('p'), Message::JumpToPreviousChange),
        (KeyCode::Char('e'), Message::ToggleFilePane),
        (KeyCode::Char(' '), Message::ToggleStageSelected),
        (KeyCode::Char('a'), Message::ToggleStageAll),
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
fn bindings_are_unique_and_generate_help() {
    for (index, binding) in KEY_BINDINGS.iter().enumerate() {
        for key in binding.keys {
            assert_eq!(
                map_key(key.code, key.required_modifiers),
                Some(binding.message.clone())
            );
        }
        for other in &KEY_BINDINGS[index + 1..] {
            assert!(
                !binding.keys.iter().any(|key| other.keys.contains(key)),
                "key chord is assigned to more than one action"
            );
        }
    }

    let rows = help_rows();
    assert!(rows.contains(&("r".to_owned(), "Toggle inline / side-by-side view")));
    assert!(rows.contains(&("e".to_owned(), "Show / hide file list")));
    assert!(rows.contains(&("Space".to_owned(), "Stage / unstage selected file")));
    assert!(rows.contains(&("j / w".to_owned(), "Previous file")));
    assert!(rows.contains(&("k / l / s".to_owned(), "Next file")));
    assert!(rows.contains(&("c".to_owned(), "Open path menu")));
    assert!(rows.contains(&("n".to_owned(), "Next change")));
    assert!(rows.contains(&("p".to_owned(), "Previous change")));
    assert!(rows.contains(&("q / Esc / Ctrl+c".to_owned(), "Quit")));
}

#[test]
fn private_diff_bindings_do_not_own_picker_navigation() {
    for code in [
        KeyCode::Char('j'),
        KeyCode::Char('w'),
        KeyCode::Char('k'),
        KeyCode::Char('l'),
        KeyCode::Char('s'),
        KeyCode::Char('g'),
        KeyCode::Char('c'),
        KeyCode::Home,
        KeyCode::End,
    ] {
        assert_eq!(map_key(code, KeyModifiers::NONE), None);
    }
}

#[test]
fn uppercase_characters_and_d_are_not_shortcuts() {
    for character in ['A', 'D', 'G', 'N', 'Q', 'd'] {
        assert_eq!(map_key(KeyCode::Char(character), KeyModifiers::NONE), None);
    }

    assert!(KEY_BINDINGS.iter().all(|binding| {
        binding
            .keys
            .iter()
            .all(|key| !matches!(key.code, KeyCode::Char(character) if character.is_uppercase()))
    }));
}

#[test]
fn maps_control_c() {
    let model = model();
    assert_eq!(
        map_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL,)),
            &model,
            Rect::default(),
        ),
        Some(Message::Quit)
    );
}

#[test]
fn focused_commit_input_keeps_control_c_as_global_quit() {
    let mut model = model();
    model.focus_commit_input();

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
            &Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
            &model,
            Rect::default(),
        ),
        Some(Message::CommitMessageInput('q'))
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
                column: 80,
                row: 1,
                modifiers: KeyModifiers::NONE,
            }),
            &model,
            Rect::new(0, 0, 100, 30),
        ),
        None
    );
}

#[test]
fn maps_commit_input_and_only_the_enabled_primary_button() {
    let mut model = model();
    let area = Rect::new(0, 0, 100, 30);
    let click = |column, row| {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    };

    assert_eq!(
        map_event(&click(2, 1), &model, area),
        Some(Message::FocusCommitInput)
    );
    assert_eq!(map_event(&click(2, 3), &model, area), None);
    model.snapshot.files[0].staged = Some(FileDiff {
        text: String::new(),
    });
    model.focus_commit_input();
    model.commit_message_input('x');
    assert_eq!(
        map_event(&click(2, 3), &model, area),
        Some(Message::BlurCommitInput),
        "a click outside the modal closes it"
    );
    model.blur_commit_input();
    assert_eq!(
        map_event(&click(2, 3), &model, area),
        Some(Message::ExecutePrimaryAction)
    );
}

#[test]
fn commit_editor_captures_mouse_and_keyboard_until_closed() {
    let mut model = model();
    model.snapshot.files[0].staged = Some(FileDiff {
        text: String::new(),
    });
    model.focus_commit_input();
    let area = Rect::new(0, 0, 100, 30);
    let click = |column, row| {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    };

    assert_eq!(map_event(&click(50, 11), &model, area), None);
    assert_eq!(
        map_event(&click(65, 14), &model, area),
        Some(Message::BlurCommitInput)
    );
    assert_eq!(
        map_event(
            &Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            &model,
            area,
        ),
        Some(Message::BlurCommitInput)
    );
    assert_eq!(
        map_event(
            &Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)),
            &model,
            area,
        ),
        Some(Message::CommitMessageInput('s'))
    );
}

#[test]
fn maps_mouse_wheel_to_diff_scrolling() {
    let model = model();
    let mouse = |kind| {
        Event::Mouse(MouseEvent {
            kind,
            column: 80,
            row: 10,
            modifiers: KeyModifiers::NONE,
        })
    };

    assert_eq!(
        map_event(&mouse(MouseEventKind::ScrollUp), &model, Rect::default()),
        Some(Message::ScrollDiffVerticalBy(-1))
    );
    assert_eq!(
        map_event(&mouse(MouseEventKind::ScrollDown), &model, Rect::default()),
        Some(Message::ScrollDiffVerticalBy(1))
    );
}
