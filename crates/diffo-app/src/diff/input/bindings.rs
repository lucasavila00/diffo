use super::{KeyCode, KeyModifiers, Message};
use diffo_ui::text_view::LINE_SCROLL_ROWS;

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
            KeyChord::plain(KeyCode::Char('q')),
            KeyChord::plain(KeyCode::Esc),
            KeyChord::control('c'),
        ],
        message: Message::Quit,
        description: "Quit",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Up)],
        message: Message::ScrollDiffVerticalBy(-LINE_SCROLL_ROWS),
        description: "Scroll diff up by four lines",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Down)],
        message: Message::ScrollDiffVerticalBy(LINE_SCROLL_ROWS),
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
        message: Message::ScrollDiffHorizontalBy(-LINE_SCROLL_ROWS),
        description: "Scroll diff left by four columns",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Right)],
        message: Message::ScrollDiffHorizontalBy(LINE_SCROLL_ROWS),
        description: "Scroll diff right by four columns",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char('r'))],
        message: Message::ToggleDiffView,
        description: "Toggle inline / side-by-side / hunk view",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char('m'))],
        message: Message::FocusCommitInput,
        description: "Edit commit message",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Char('i'))],
        message: Message::ExecuteAiCommit,
        description: "AI commit staged changes",
    },
    KeyBinding {
        keys: &[KeyChord::plain(KeyCode::Enter)],
        message: Message::ExecuteCommit,
        description: "Commit staged changes or complete merge",
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
    std::iter::once(("f".to_owned(), "Toggle full-screen buffer"))
        .chain(
            KEY_BINDINGS
                .iter()
                .filter(|binding| binding.message != Message::JumpToPreviousChange)
                .map(|binding| {
                    if binding.message == Message::JumpToNextChange {
                        return ("n / p".to_owned(), "Next / previous change");
                    }
                    let keys = binding
                        .keys
                        .iter()
                        .map(|key| key.label())
                        .collect::<Vec<_>>()
                        .join(" / ");
                    (keys, binding.description)
                }),
        )
        .collect()
}

pub(crate) fn review_help_rows() -> Vec<(String, &'static str)> {
    std::iter::once(("f".to_owned(), "Toggle full-screen buffer"))
        .chain(
            KEY_BINDINGS
                .iter()
                .filter(|binding| super::is_review_message(&binding.message))
                .filter(|binding| binding.message != Message::JumpToPreviousChange)
                .map(|binding| {
                    if binding.message == Message::JumpToNextChange {
                        return ("n / p".to_owned(), "Next / previous change");
                    }
                    let keys = binding
                        .keys
                        .iter()
                        .map(|key| key.label())
                        .collect::<Vec<_>>()
                        .join(" / ");
                    (keys, binding.description)
                }),
        )
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
