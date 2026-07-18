use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use diffo_app::{ChangeArea, FileKey, Message, Model};
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
        (KeyCode::Char('1'), Message::OpenCommandPalette),
        (KeyCode::F(1), Message::OpenCommandPalette),
        (KeyCode::Char('2'), Message::ToggleHelp),
        (KeyCode::F(2), Message::ToggleHelp),
        (KeyCode::Esc, Message::Quit),
        (KeyCode::Char('j'), Message::SelectPreviousFile),
        (KeyCode::Char('w'), Message::SelectPreviousFile),
        (KeyCode::Char('k'), Message::SelectNextFile),
        (KeyCode::Char('l'), Message::SelectNextFile),
        (KeyCode::Char('s'), Message::SelectNextFile),
        (KeyCode::Up, Message::ScrollDiffUp),
        (KeyCode::Down, Message::ScrollDiffDown),
        (KeyCode::PageUp, Message::ScrollDiffPageUp(1)),
        (KeyCode::PageDown, Message::ScrollDiffPageDown(1)),
        (KeyCode::Left, Message::ScrollDiffLeft),
        (KeyCode::Right, Message::ScrollDiffRight),
        (KeyCode::Char('r'), Message::ToggleDiffView),
        (KeyCode::Char('n'), Message::JumpToNextChange),
        (KeyCode::Char('e'), Message::ToggleFilePane),
        (KeyCode::Home, Message::SelectFirstFile),
        (KeyCode::End, Message::SelectLastFile),
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
    assert!(rows.contains(&("q / Esc / Ctrl+c".to_owned(), "Quit")));
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
                row: 19,
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
    assert_eq!(
        map_event(
            &Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Right),
                column: 4,
                row: 19,
                modifiers: KeyModifiers::NONE,
            }),
            &model,
            Rect::new(0, 0, 100, 30),
        ),
        Some(Message::OpenFileContextMenu(
            FileKey {
                path: PathBuf::from("file.txt"),
                area: ChangeArea::Unstaged,
            },
            4,
            19,
        ))
    );
}

#[test]
fn command_palette_captures_keys_and_escape_closes_it() {
    let mut model = model();
    model.open_command_palette();

    for (key, expected) in [
        (KeyCode::Char('q'), Message::CommandPaletteInput('q')),
        (KeyCode::Backspace, Message::CommandPaletteBackspace),
        (KeyCode::Up, Message::CommandPaletteSelectPrevious),
        (KeyCode::Down, Message::CommandPaletteSelectNext),
        (KeyCode::Enter, Message::ExecuteSelectedCommand),
        (KeyCode::Esc, Message::CloseCommandPalette),
    ] {
        assert_eq!(
            map_event(
                &Event::Key(KeyEvent::new(key, KeyModifiers::NONE)),
                &model,
                Rect::default()
            ),
            Some(expected)
        );
    }
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
fn maps_file_pane_dragging() {
    let mut model = model();
    let area = Rect::new(0, 0, 100, 30);
    let down = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 25,
        row: 5,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(
        map_event(&down, &model, area),
        Some(Message::BeginFilePaneResize)
    );

    model.resizing_file_pane = true;
    let drag = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 60,
        row: 5,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(
        map_event(&drag, &model, area),
        Some(Message::ResizeFilePane(60))
    );
    let up = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 60,
        row: 5,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(
        map_event(&up, &model, area),
        Some(Message::EndFilePaneResize)
    );
}

#[test]
fn maps_file_action_buttons() {
    let mut model = model();
    model.snapshot.files[0].kind = ChangeKind::Modified;
    model.snapshot.files[0].staged = Some(FileDiff {
        text: String::new(),
    });
    model.snapshot.files[0].unstaged = Some(FileDiff {
        text: String::new(),
    });
    let area = Rect::new(0, 0, 100, 30);
    let click = |row| {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 22,
            row,
            modifiers: KeyModifiers::NONE,
        })
    };

    assert_eq!(
        map_event(&click(7), &model, area),
        Some(Message::UnstageFile(PathBuf::from("file.txt")))
    );
    assert_eq!(
        map_event(&click(19), &model, area),
        Some(Message::StageFile(PathBuf::from("file.txt")))
    );
    assert_eq!(
        map_event(
            &Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 11,
                row: 18,
                modifiers: KeyModifiers::NONE,
            }),
            &model,
            area,
        ),
        Some(Message::StageAll)
    );
    assert_eq!(
        map_event(
            &Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 13,
                row: 18,
                modifiers: KeyModifiers::NONE,
            }),
            &model,
            area,
        ),
        None,
        "the Stage All label must not be clickable"
    );
    assert_eq!(
        map_event(
            &Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 10,
                row: 6,
                modifiers: KeyModifiers::NONE,
            }),
            &model,
            area,
        ),
        Some(Message::UnstageAll)
    );
    assert_eq!(
        map_event(
            &Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 13,
                row: 6,
                modifiers: KeyModifiers::NONE,
            }),
            &model,
            area,
        ),
        None,
        "the Unstage All label must not be clickable"
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
        Some(Message::ScrollDiffBy(-1))
    );
    assert_eq!(
        map_event(&mouse(MouseEventKind::ScrollDown), &model, Rect::default()),
        Some(Message::ScrollDiffBy(1))
    );
}
