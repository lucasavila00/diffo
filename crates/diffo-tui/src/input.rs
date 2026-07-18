use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_app::{Message, Model};
use diffo_core::AccessMode;
use ratatui::layout::Rect;

use crate::{
    commit_action_at_position, commit_editor_action_at_position, file_action_at_position,
    file_at_position, file_pane_percent_at, is_file_pane_splitter_at,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Availability {
    Always,
    ReadWrite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct KeyChord {
    code: KeyCode,
    required_modifiers: KeyModifiers,
}

impl KeyChord {
    const fn plain(code: KeyCode) -> Self {
        Self {
            code,
            required_modifiers: KeyModifiers::NONE,
        }
    }

    const fn control(character: char) -> Self {
        Self {
            code: KeyCode::Char(character),
            required_modifiers: KeyModifiers::CONTROL,
        }
    }

    fn matches(self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        self.code == code && modifiers.contains(self.required_modifiers)
    }
}

struct KeyBinding {
    keys: &'static [KeyChord],
    message: Message,
    description: &'static str,
    availability: Availability,
}

static KEY_BINDINGS: &[KeyBinding] = &[
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Char('1')),
            KeyChord::plain(KeyCode::F(1)),
        ],
        message: Message::OpenCommandPalette,
        description: "Open command palette",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Char('2')),
            KeyChord::plain(KeyCode::F(2)),
        ],
        message: Message::ToggleHelp,
        description: "Toggle help",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Char('q')),
            KeyChord::plain(KeyCode::Esc),
            KeyChord::control('c'),
        ],
        message: Message::Quit,
        description: "Quit",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('w')),
        ],
        message: Message::SelectPreviousFile,
        description: "Previous file",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('l')),
            KeyChord::plain(KeyCode::Char('s')),
        ],
        message: Message::SelectNextFile,
        description: "Next file",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Up)],
        message: Message::ScrollDiffUp,
        description: "Scroll diff up by four lines",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Down)],
        message: Message::ScrollDiffDown,
        description: "Scroll diff down by four lines",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::PageUp)],
        message: Message::ScrollDiffPageUp(0),
        description: "Scroll up one page",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::PageDown)],
        message: Message::ScrollDiffPageDown(0),
        description: "Scroll down one page",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Left)],
        message: Message::ScrollDiffLeft,
        description: "Scroll diff left by four columns",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Right),
            KeyChord::plain(KeyCode::Char('d')),
        ],
        message: Message::ScrollDiffRight,
        description: "Scroll diff right by four columns",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char('r'))],
        message: Message::ToggleDiffView,
        description: "Toggle inline / side-by-side view",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char('N'))],
        message: Message::JumpToPreviousChange,
        description: "Previous change",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char('n'))],
        message: Message::JumpToNextChange,
        description: "Next change",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char('e'))],
        message: Message::ToggleFilePane,
        description: "Show / hide file list",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Home),
            KeyChord::plain(KeyCode::Char('g')),
        ],
        message: Message::SelectFirstFile,
        description: "First file",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::End),
            KeyChord::plain(KeyCode::Char('G')),
        ],
        message: Message::SelectLastFile,
        description: "Last file",
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char(' '))],
        message: Message::ToggleStageSelected,
        description: "Stage / unstage selected file",
        availability: Availability::ReadWrite,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char('a'))],
        message: Message::ToggleStageAll,
        description: "Stage / unstage all files",
        availability: Availability::ReadWrite,
    },
];

pub(crate) fn help_rows(access_mode: AccessMode) -> Vec<(String, &'static str)> {
    KEY_BINDINGS
        .iter()
        .filter(|binding| binding.is_available(access_mode))
        .map(|binding| {
            let keys = binding
                .keys
                .iter()
                .map(|key| key.label())
                .collect::<Vec<_>>()
                .join(" / ");
            (keys, binding.description)
        })
        .collect()
}

impl KeyChord {
    fn label(self) -> String {
        let key = match self.code {
            KeyCode::Char(' ') => "Space".to_owned(),
            KeyCode::Char(character) => character.to_string(),
            KeyCode::Esc => "Esc".to_owned(),
            KeyCode::Up => "↑".to_owned(),
            KeyCode::Down => "↓".to_owned(),
            KeyCode::Left => "←".to_owned(),
            KeyCode::Right => "→".to_owned(),
            KeyCode::Home => "Home".to_owned(),
            KeyCode::End => "End".to_owned(),
            KeyCode::PageUp => "Page Up".to_owned(),
            KeyCode::PageDown => "Page Down".to_owned(),
            KeyCode::F(number) => format!("F{number}"),
            other => format!("{other:?}"),
        };
        if self.required_modifiers.contains(KeyModifiers::CONTROL) {
            format!("Ctrl+{key}")
        } else {
            key
        }
    }
}

impl KeyBinding {
    fn is_available(&self, access_mode: AccessMode) -> bool {
        self.availability == Availability::Always || access_mode == AccessMode::ReadWrite
    }
}

#[must_use]
pub fn map_event(event: &Event, model: &Model, area: Rect) -> Option<Message> {
    if model.help_open {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && matches!(key.code, KeyCode::Char('2') | KeyCode::F(2))
        {
            return Some(Message::ToggleHelp);
        }
        return map_help_event(event);
    }
    if model.command_palette.is_some() {
        return map_command_palette_event(event);
    }
    if model.commit_input_focused() {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                return Some(Message::Quit);
            }
            return match key.code {
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
            };
        }
        return match event {
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                commit_editor_action_at_position(model, area, mouse.column, mouse.row)
            }
            _ => None,
        };
    }
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            map_key(key.code, key.modifiers, model.access_mode).map(|message| {
                let page_lines = usize::from(area.height.saturating_sub(3)).max(1);
                match message {
                    Message::ScrollDiffPageUp(_) => Message::ScrollDiffPageUp(page_lines),
                    Message::ScrollDiffPageDown(_) => Message::ScrollDiffPageDown(page_lines),
                    other => other,
                }
            })
        }
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

fn map_help_event(event: &Event) -> Option<Message> {
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

fn map_command_palette_event(event: &Event) -> Option<Message> {
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

fn map_key(code: KeyCode, modifiers: KeyModifiers, access_mode: AccessMode) -> Option<Message> {
    KEY_BINDINGS
        .iter()
        .filter(|binding| binding.is_available(access_mode))
        .find(|binding| binding.keys.iter().any(|key| key.matches(code, modifiers)))
        .map(|binding| binding.message.clone())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton,
        MouseEvent, MouseEventKind,
    };
    use diffo_app::{ChangeArea, FileKey, Message, Model};
    use diffo_core::{AccessMode, ChangeKind, FileDiff, FileState, RepositorySnapshot};
    use ratatui::layout::Rect;

    use super::{KEY_BINDINGS, help_rows, map_event, map_key};

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
            (KeyCode::Char('d'), Message::ScrollDiffRight),
            (KeyCode::Char('r'), Message::ToggleDiffView),
            (KeyCode::Char('n'), Message::JumpToNextChange),
            (KeyCode::Char('N'), Message::JumpToPreviousChange),
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
                    map_key(key.code, key.required_modifiers, AccessMode::ReadWrite),
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

        let read_write = help_rows(AccessMode::ReadWrite);
        assert!(read_write.contains(&("r".to_owned(), "Toggle inline / side-by-side view")));
        assert!(read_write.contains(&("e".to_owned(), "Show / hide file list")));
        assert!(read_write.contains(&("Space".to_owned(), "Stage / unstage selected file")));
        assert!(read_write.contains(&("j / w".to_owned(), "Previous file")));
        assert!(read_write.contains(&("k / l / s".to_owned(), "Next file")));
        assert!(read_write.contains(&("q / Esc / Ctrl+c".to_owned(), "Quit")));

        let read_only = help_rows(AccessMode::ReadOnly);
        assert!(read_only.contains(&("r".to_owned(), "Toggle inline / side-by-side view")));
        assert!(read_only.contains(&("e".to_owned(), "Show / hide file list")));
        assert!(!read_only.iter().any(|(keys, _)| keys == "Space"));
    }

    #[test]
    fn read_only_mode_does_not_dispatch_mutations() {
        let mut model = model();
        model.access_mode = AccessMode::ReadOnly;

        for key in [' ', 'a'] {
            assert_eq!(
                map_event(
                    &Event::Key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)),
                    &model,
                    Rect::default(),
                ),
                None
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
}
