use super::{KeyCode, KeyModifiers, Message};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct KeyChord {
    pub(super) code: KeyCode,
    pub(super) required_modifiers: KeyModifiers,
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

    pub(super) fn matches(self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        self.code == code && modifiers.contains(self.required_modifiers)
    }
}

pub(super) struct KeyBinding {
    pub(super) keys: &'static [KeyChord],
    pub(super) message: Message,
    pub(super) description: &'static str,
}

pub(super) static KEY_BINDINGS: &[KeyBinding] = &[
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Char('1')),
            KeyChord::plain(KeyCode::F(1)),
        ],
        message: Message::OpenCommandPalette,
        description: "Open command palette",
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Char('2')),
            KeyChord::plain(KeyCode::F(2)),
        ],
        message: Message::ToggleHelp,
        description: "Toggle help",
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Char('q')),
            KeyChord::plain(KeyCode::Esc),
            KeyChord::control('c'),
        ],
        message: Message::Quit,
        description: "Quit",
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Char('j')),
            KeyChord::plain(KeyCode::Char('w')),
        ],
        message: Message::SelectPreviousFile,
        description: "Previous file",
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Char('k')),
            KeyChord::plain(KeyCode::Char('l')),
            KeyChord::plain(KeyCode::Char('s')),
        ],
        message: Message::SelectNextFile,
        description: "Next file",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Up)],
        message: Message::ScrollDiffUp,
        description: "Scroll diff up by four lines",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Down)],
        message: Message::ScrollDiffDown,
        description: "Scroll diff down by four lines",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::PageUp)],
        message: Message::ScrollDiffPageUp(0),
        description: "Scroll up one page",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::PageDown)],
        message: Message::ScrollDiffPageDown(0),
        description: "Scroll down one page",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Left)],
        message: Message::ScrollDiffLeft,
        description: "Scroll diff left by four columns",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Right)],
        message: Message::ScrollDiffRight,
        description: "Scroll diff right by four columns",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char('r'))],
        message: Message::ToggleDiffView,
        description: "Toggle inline / side-by-side view",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char('n'))],
        message: Message::JumpToNextChange,
        description: "Next change",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char('p'))],
        message: Message::JumpToPreviousChange,
        description: "Previous change",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char('e'))],
        message: Message::ToggleFilePane,
        description: "Show / hide file list",
    },
    KeyBinding {
        keys: &[
            KeyChord::plain(KeyCode::Home),
            KeyChord::plain(KeyCode::Char('g')),
        ],
        message: Message::SelectFirstFile,
        description: "First file",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::End)],
        message: Message::SelectLastFile,
        description: "Last file",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char(' '))],
        message: Message::ToggleStageSelected,
        description: "Stage / unstage selected file",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char('a'))],
        message: Message::ToggleStageAll,
        description: "Stage / unstage all files",
    },
];

pub(crate) fn help_rows() -> Vec<(String, &'static str)> {
    KEY_BINDINGS
        .iter()
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
