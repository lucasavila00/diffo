use diffo_ui::text_view::{LINE_SCROLL_ROWS, ScrollCommand, ViewportMetrics};

use super::{DiffViewMode, FramePreparation, Message, Renderer, ViewportTransition};

/// View mode and committed viewport shared by every right-side review pane.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReviewState {
    pub diff_scroll: usize,
    pub diff_horizontal_scroll: usize,
    pub diff_view_mode: DiffViewMode,
    maximum_vertical: usize,
    maximum_horizontal: usize,
}

impl Renderer {
    pub(in crate::diff) fn vertical_message(
        &mut self,
        message: Message,
        state: &ReviewState,
    ) -> Message {
        let command = match message {
            Message::SetDiffScroll(target) => ScrollCommand::Vertical(target),
            Message::ScrollDiffUp => ScrollCommand::Lines(-LINE_SCROLL_ROWS),
            Message::ScrollDiffDown => ScrollCommand::Lines(LINE_SCROLL_ROWS),
            Message::ScrollDiffPageUp(lines) => {
                ScrollCommand::Lines(-i64::try_from(lines).unwrap_or(i64::MAX))
            }
            Message::ScrollDiffPageDown(lines) => {
                ScrollCommand::Lines(i64::try_from(lines).unwrap_or(i64::MAX))
            }
            Message::ScrollDiffVerticalBy(lines) => ScrollCommand::Lines(lines),
            _ => return message,
        };
        let target = self
            .vertical_scroll
            .request(
                command,
                state.diff_scroll,
                ViewportMetrics {
                    maximum_vertical: usize::MAX,
                    ..ViewportMetrics::default()
                },
            )
            .unwrap_or(state.diff_scroll);
        Message::JumpDiffToPosition(target)
    }
}

impl ReviewState {
    pub(crate) fn apply_preparation(&mut self, preparation: &FramePreparation) {
        if let Some(ViewportTransition {
            vertical,
            horizontal,
        }) = preparation.viewport_transition
        {
            self.set_viewport(vertical, horizontal);
        }
        self.maximum_vertical = preparation.maximum_vertical_scroll;
        self.maximum_horizontal = preparation.maximum_horizontal_scroll;
        self.clamp();
    }

    pub(crate) fn update(&mut self, message: &Message) -> bool {
        let before = self.clone();
        match message {
            Message::ScrollDiffUp => self.scroll_vertical_by(-4),
            Message::ScrollDiffDown => self.scroll_vertical_by(4),
            Message::ScrollDiffPageUp(lines) => {
                self.scroll_vertical_by(-i64::try_from(*lines).unwrap_or(i64::MAX));
            }
            Message::ScrollDiffPageDown(lines) => {
                self.scroll_vertical_by(i64::try_from(*lines).unwrap_or(i64::MAX));
            }
            Message::ScrollDiffVerticalBy(lines) => self.scroll_vertical_by(*lines),
            Message::SetDiffScroll(position) => self.diff_scroll = *position,
            Message::SetDiffHorizontalScroll(position) => {
                self.diff_horizontal_scroll = *position;
            }
            Message::ScrollDiffLeft => self.scroll_horizontal_by(-4),
            Message::ScrollDiffRight => self.scroll_horizontal_by(4),
            Message::ScrollDiffHorizontalBy(columns) => self.scroll_horizontal_by(*columns),
            Message::ToggleDiffView => self.diff_view_mode = self.diff_view_mode.toggled(),
            Message::JumpDiffToPosition(_)
            | Message::JumpToPreviousChange
            | Message::JumpToNextChange => {}
            _ => return false,
        }
        *self != before
    }

    fn scroll_vertical_by(&mut self, lines: i64) {
        let magnitude = usize::try_from(lines.unsigned_abs()).unwrap_or(usize::MAX);
        self.diff_scroll = if lines < 0 {
            self.diff_scroll.saturating_sub(magnitude)
        } else {
            self.diff_scroll.saturating_add(magnitude)
        };
    }

    fn scroll_horizontal_by(&mut self, columns: i64) {
        let magnitude = usize::try_from(columns.unsigned_abs()).unwrap_or(usize::MAX);
        self.diff_horizontal_scroll = if columns < 0 {
            self.diff_horizontal_scroll.saturating_sub(magnitude)
        } else {
            self.diff_horizontal_scroll.saturating_add(magnitude)
        };
    }

    pub(crate) fn clamp(&mut self) {
        self.diff_scroll = self.diff_scroll.min(self.maximum_vertical);
        self.diff_horizontal_scroll = self.diff_horizontal_scroll.min(self.maximum_horizontal);
    }

    pub(crate) fn set_viewport(&mut self, vertical: usize, horizontal: usize) {
        self.diff_scroll = vertical;
        self.diff_horizontal_scroll = horizontal;
    }
}
