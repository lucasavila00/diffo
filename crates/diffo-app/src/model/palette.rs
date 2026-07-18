use super::*;

impl Model {
    pub fn open_command_palette(&mut self) {
        self.help_open = false;
        self.command_palette = Some(CommandPalette::default());
    }

    pub fn close_command_palette(&mut self) {
        self.command_palette = None;
    }

    pub fn toggle_help(&mut self) {
        self.command_palette = None;
        self.help_open = !self.help_open;
    }

    pub fn close_help(&mut self) {
        self.help_open = false;
    }

    pub fn command_palette_input(&mut self, character: char) {
        if let Some(palette) = self.command_palette.as_mut() {
            palette.push(character);
        }
    }

    pub fn command_palette_backspace(&mut self) {
        if let Some(palette) = self.command_palette.as_mut() {
            palette.backspace();
        }
    }

    pub fn command_palette_select_previous(&mut self) {
        if let Some(palette) = self.command_palette.as_mut() {
            palette.select_previous();
        }
    }

    pub fn command_palette_select_next(&mut self) {
        if let Some(palette) = self.command_palette.as_mut() {
            palette.select_next();
        }
    }

    pub fn command_palette_select(&mut self, index: usize) {
        if let Some(palette) = self.command_palette.as_mut() {
            palette.select(index);
        }
    }

    pub fn execute_selected_command(&mut self) -> Option<RepositoryAction> {
        if self.access_mode == AccessMode::ReadOnly || self.pending_operation.is_some() {
            return None;
        }
        let command = self.command_palette.as_ref()?.selected_command()?.id;
        self.command_palette = None;
        let action = match command {
            CommandId::Fetch => RepositoryAction::Fetch,
            CommandId::Pull => RepositoryAction::Pull,
        };
        self.error = None;
        self.pending_operation = Some(action.clone());
        Some(action)
    }
}
