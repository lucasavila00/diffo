use crate::diff::Message;

use super::Workbench;

#[derive(Default)]
pub(super) struct PendingScroll {
    vertical: i64,
    horizontal: i64,
}

impl PendingScroll {
    pub(super) fn push(&mut self, message: &Message) -> bool {
        match message {
            Message::ScrollDiffUp => self.vertical = self.vertical.saturating_sub(4),
            Message::ScrollDiffDown => self.vertical = self.vertical.saturating_add(4),
            Message::ScrollDiffPageUp(lines) => {
                self.vertical = self
                    .vertical
                    .saturating_sub(i64::try_from(*lines).unwrap_or(i64::MAX));
            }
            Message::ScrollDiffPageDown(lines) => {
                self.vertical = self
                    .vertical
                    .saturating_add(i64::try_from(*lines).unwrap_or(i64::MAX));
            }
            Message::ScrollDiffVerticalBy(lines) => {
                self.vertical = self.vertical.saturating_add(*lines);
            }
            Message::ScrollDiffLeft => self.horizontal = self.horizontal.saturating_sub(4),
            Message::ScrollDiffRight => self.horizontal = self.horizontal.saturating_add(4),
            Message::ScrollDiffHorizontalBy(columns) => {
                self.horizontal = self.horizontal.saturating_add(*columns);
            }
            _ => return false,
        }
        true
    }

    pub(super) fn flush(&mut self, workbench: &mut Workbench) {
        if self.vertical != 0 {
            let _ = workbench.update_diff(Message::ScrollDiffVerticalBy(self.vertical));
        }
        if self.horizontal != 0 {
            let _ = workbench.update_diff(Message::ScrollDiffHorizontalBy(self.horizontal));
        }
        *self = Self::default();
    }
}
