use super::{Model, NetworkOperation, RepositoryAction};

impl Model {
    pub fn commit_message_input(&mut self, character: char) {
        if !self.ai_commit_pending && !character.is_control() {
            let byte = byte_index_at_char(&self.commit_message, self.commit_message_cursor);
            self.commit_message.insert(byte, character);
            self.commit_message_cursor = self.commit_message_cursor.saturating_add(1);
        }
    }

    pub fn commit_message_backspace(&mut self) {
        if !self.ai_commit_pending && self.commit_message_cursor > 0 {
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
        if self.ai_commit_pending {
            return;
        }
        self.commit_message_cursor = self.commit_message_cursor.saturating_sub(1);
    }

    pub fn commit_message_cursor_right(&mut self) {
        if self.ai_commit_pending {
            return;
        }
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
    pub fn commit_enabled(&self) -> bool {
        !self.ai_commit_pending
            && self.pending_operation.is_none()
            && self.effective_commit_message().is_some()
    }

    #[must_use]
    pub fn sync_enabled(&self) -> bool {
        !self.ai_commit_pending && self.pending_operation.is_none()
    }

    #[must_use]
    pub const fn ai_commit_pending(&self) -> bool {
        self.ai_commit_pending
    }

    pub fn begin_ai_commit(&mut self) {
        self.ai_commit_pending = true;
    }

    pub fn finish_ai_commit(&mut self) {
        self.ai_commit_pending = false;
    }

    pub fn install_generated_commit_message(&mut self, message: String) {
        self.commit_message_cursor = message.chars().count();
        self.commit_message = message;
    }

    pub fn execute_commit(&mut self) -> Option<RepositoryAction> {
        if !self.commit_enabled() {
            return None;
        }
        let action = RepositoryAction::Commit(self.effective_commit_message()?);
        self.pending_operation = Some(action.clone());
        Some(action)
    }

    pub fn execute_sync(&mut self) -> Option<RepositoryAction> {
        self.start_sync(RepositoryAction::Sync)
    }

    pub fn execute_sync_to_remote(&mut self, remote: String) -> Option<RepositoryAction> {
        self.start_sync(RepositoryAction::SyncToRemote(remote))
    }

    fn start_sync(&mut self, action: RepositoryAction) -> Option<RepositoryAction> {
        if !self.sync_enabled() {
            return None;
        }
        self.pending_operation = Some(action.clone());
        Some(action)
    }

    #[must_use]
    pub fn network_operation(&self) -> Option<NetworkOperation> {
        match self.pending_operation.as_ref() {
            Some(RepositoryAction::Fetch) => Some(NetworkOperation::Fetch),
            Some(RepositoryAction::Sync | RepositoryAction::SyncToRemote(_)) => {
                Some(NetworkOperation::Sync)
            }
            _ => None,
        }
    }
}

pub(super) fn byte_index_at_char(text: &str, character: usize) -> usize {
    text.char_indices()
        .nth(character)
        .map_or(text.len(), |(index, _)| index)
}
