use super::{Model, NetworkOperation, PrimaryAction, RepositoryAction};

impl Model {
    pub fn commit_message_input(&mut self, character: char) {
        if !character.is_control() {
            let byte = byte_index_at_char(&self.commit_message, self.commit_message_cursor);
            self.commit_message.insert(byte, character);
            self.commit_message_cursor = self.commit_message_cursor.saturating_add(1);
        }
    }

    pub fn commit_message_backspace(&mut self) {
        if self.commit_message_cursor > 0 {
            let start = byte_index_at_char(
                &self.commit_message,
                self.commit_message_cursor.saturating_sub(1),
            );
            let end = byte_index_at_char(&self.commit_message, self.commit_message_cursor);
            self.commit_message.replace_range(start..end, "");
            self.commit_message_cursor = self.commit_message_cursor.saturating_sub(1);
        }
    }

    pub fn commit_message_cursor_left(&mut self) {
        self.commit_message_cursor = self.commit_message_cursor.saturating_sub(1);
    }

    pub fn commit_message_cursor_right(&mut self) {
        self.commit_message_cursor = self
            .commit_message_cursor
            .saturating_add(1)
            .min(self.commit_message.chars().count());
    }

    #[must_use]
    pub fn commit_message_cursor(&self) -> usize {
        self.commit_message_cursor
    }

    #[must_use]
    pub fn suggested_commit_message(&self) -> Option<String> {
        let staged_files = self
            .snapshot
            .files
            .iter()
            .filter(|file| file.staged.is_some())
            .count();
        match staged_files {
            0 => None,
            1 => Some("Update 1 file".to_owned()),
            count => Some(format!("Update {count} files")),
        }
    }

    pub(super) fn effective_commit_message(&self) -> Option<String> {
        let message = self.commit_message.trim();
        if message.is_empty() {
            self.suggested_commit_message()
        } else {
            Some(message.to_owned())
        }
    }

    #[must_use]
    pub fn primary_action(&self) -> PrimaryAction {
        if let Some(action) = self.pending_operation.as_ref() {
            return match action {
                RepositoryAction::Commit(_) => PrimaryAction::Commit,
                RepositoryAction::Push => PrimaryAction::Push,
                RepositoryAction::Pull => PrimaryAction::Pull,
                _ => PrimaryAction::Disabled,
            };
        }
        if self.effective_commit_message().is_some() {
            return PrimaryAction::Commit;
        }
        match self.snapshot.upstream.as_ref() {
            Some(upstream) if upstream.ahead > 0 && upstream.behind > 0 => {
                PrimaryAction::PushAndPull
            }
            Some(upstream) if upstream.behind > 0 => PrimaryAction::Pull,
            Some(upstream) if upstream.ahead > 0 => PrimaryAction::Push,
            _ => PrimaryAction::Disabled,
        }
    }

    #[must_use]
    pub fn primary_action_enabled(&self) -> bool {
        self.pending_operation.is_none() && self.primary_action().enabled()
    }

    pub fn execute_primary_action(&mut self) -> Option<RepositoryAction> {
        let primary = self.primary_action();
        if primary == PrimaryAction::PushAndPull {
            return None;
        }
        if !self.primary_action_enabled() {
            return None;
        }
        let action = match primary {
            PrimaryAction::Commit => RepositoryAction::Commit(self.effective_commit_message()?),
            PrimaryAction::Push => RepositoryAction::Push,
            PrimaryAction::Pull => RepositoryAction::Pull,
            PrimaryAction::PushAndPull | PrimaryAction::Disabled => return None,
        };
        self.error = None;
        self.pending_operation = Some(action.clone());
        Some(action)
    }

    #[must_use]
    pub fn network_operation(&self) -> Option<NetworkOperation> {
        match self.pending_operation.as_ref() {
            Some(RepositoryAction::Fetch) => Some(NetworkOperation::Fetch),
            Some(RepositoryAction::Pull) => Some(NetworkOperation::Pull),
            Some(RepositoryAction::Push) => Some(NetworkOperation::Push),
            _ => None,
        }
    }
}

pub(super) fn byte_index_at_char(text: &str, character: usize) -> usize {
    text.char_indices()
        .nth(character)
        .map_or(text.len(), |(index, _)| index)
}
