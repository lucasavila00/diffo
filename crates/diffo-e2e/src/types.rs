#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    Char(char),
    Tab,
    Enter,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Function(u8),
    Ctrl(char),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Selector {
    Text(String),
    PanelAction {
        panel: String,
        action: String,
    },
    FileAction {
        panel: String,
        path: String,
        action: String,
    },
    SelectedRow(String),
    DialogAction {
        dialog: String,
        action: String,
    },
    ToastAction {
        toast: String,
        action: String,
    },
    VerticalScrollbarEnd,
}

impl Selector {
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    #[must_use]
    pub fn panel_action(panel: impl Into<String>, action: impl Into<String>) -> Self {
        Self::PanelAction {
            panel: panel.into(),
            action: action.into(),
        }
    }

    #[must_use]
    pub fn file_action(
        panel: impl Into<String>,
        path: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self::FileAction {
            panel: panel.into(),
            path: path.into(),
            action: action.into(),
        }
    }

    #[must_use]
    pub fn selected_row(text: impl Into<String>) -> Self {
        Self::SelectedRow(text.into())
    }

    #[must_use]
    pub fn dialog_action(dialog: impl Into<String>, action: impl Into<String>) -> Self {
        Self::DialogAction {
            dialog: dialog.into(),
            action: action.into(),
        }
    }

    #[must_use]
    pub fn toast_action(toast: impl Into<String>, action: impl Into<String>) -> Self {
        Self::ToastAction {
            toast: toast.into(),
            action: action.into(),
        }
    }

    #[must_use]
    pub const fn vertical_scrollbar_end() -> Self {
        Self::VerticalScrollbarEnd
    }
}
