use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_app::{Message, Model};
use diffo_core::AccessMode;
use ratatui::layout::Rect;

use crate::{
    file_action_at_position, file_at_position, file_pane_percent_at, is_file_pane_splitter_at,
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
    help: Option<&'static str>,
    availability: Availability,
}

static KEY_BINDINGS: &[KeyBinding] = &[
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Char('1')),
            KeyChord::plain(KeyCode::F(1)),
        ],
        message: Message::OpenCommandPalette,
        help: Some("1/f1: commands"),
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Char('2')),
            KeyChord::plain(KeyCode::F(2)),
        ],
        message: Message::ToggleHelp,
        help: Some("2/f2: help"),
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Char('q')),
            KeyChord::plain(KeyCode::Esc),
            KeyChord::control('c'),
        ],
        message: Message::Quit,
        help: Some("q: quit"),
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('w')),
        ],
        message: Message::SelectPreviousFile,
        help: Some("j/w: previous"),
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('l')),
            KeyChord::plain(KeyCode::Char('s')),
        ],
        message: Message::SelectNextFile,
        help: Some("k/l/s: next"),
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Up)],
        message: Message::ScrollDiffUp,
        help: Some("arrows: scroll"),
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Down)],
        message: Message::ScrollDiffDown,
        help: None,
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::PageUp)],
        message: Message::ScrollDiffPageUp(0),
        help: Some("pgup/pgdn: page"),
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::PageDown)],
        message: Message::ScrollDiffPageDown(0),
        help: None,
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Left)],
        message: Message::ScrollDiffLeft,
        help: None,
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Right),
            KeyChord::plain(KeyCode::Char('d')),
        ],
        message: Message::ScrollDiffRight,
        help: None,
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char('r'))],
        message: Message::ToggleDiffView,
        help: Some("r: view"),
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char('e'))],
        message: Message::ToggleFilePane,
        help: Some("e: pane"),
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Home),
            KeyChord::plain(KeyCode::Char('g')),
        ],
        message: Message::SelectFirstFile,
        help: None,
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::End),
            KeyChord::plain(KeyCode::Char('G')),
        ],
        message: Message::SelectLastFile,
        help: None,
        availability: Availability::Always,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char(' '))],
        message: Message::ToggleStageSelected,
        help: Some("space: stage/unstage"),
        availability: Availability::ReadWrite,
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char('a'))],
        message: Message::ToggleStageAll,
        help: Some("a: stage/unstage all"),
        availability: Availability::ReadWrite,
    },
];

pub(crate) fn help_text(access_mode: AccessMode) -> String {
    let mut help = KEY_BINDINGS
        .iter()
        .filter(|binding| binding.is_available(access_mode))
        .filter_map(|binding| binding.help)
        .collect::<Vec<_>>()
        .join("  ");
    if access_mode == AccessMode::ReadOnly {
        help.push_str("  read-only");
    }
    format!(" {help} ")
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
            if is_file_pane_splitter_at(model, area, mouse.column, mouse.row) {
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
            Some(Message::ScrollDiffUp)
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::ScrollDown => {
            Some(Message::ScrollDiffDown)
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

    use super::{KEY_BINDINGS, help_text, map_event, map_key};

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

        let read_write = help_text(AccessMode::ReadWrite);
        assert!(read_write.contains("r: view"));
        assert!(read_write.contains("e: pane"));
        assert!(read_write.contains("space: stage/unstage"));
        assert!(read_write.contains("j/w: previous"));
        assert!(read_write.contains("k/l/s: next"));

        let read_only = help_text(AccessMode::ReadOnly);
        assert!(read_only.contains("r: view"));
        assert!(read_only.contains("e: pane"));
        assert!(!read_only.contains("space: stage/unstage"));
        assert!(read_only.contains("read-only"));
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
                    row: 16,
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
            map_event(&click(1), &model, area),
            Some(Message::UnstageFile(PathBuf::from("file.txt")))
        );
        assert_eq!(
            map_event(&click(16), &model, area),
            Some(Message::StageFile(PathBuf::from("file.txt")))
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
            Some(Message::ScrollDiffUp)
        );
        assert_eq!(
            map_event(&mouse(MouseEventKind::ScrollDown), &model, Rect::default()),
            Some(Message::ScrollDiffDown)
        );
    }
}
