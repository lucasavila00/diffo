use super::{
    Activity, Command, CommandId, DiffActivity, Event, ExplorerActivity, ExplorerEvent, Frame,
    FramePreparation, HistoryActivity, HistoryEvent, KeyCode, KeyEventKind, KeyModifiers,
    PaneSplit, Rect, RendererEvent, TextRenderMode, TextSurfacePreparation, Tool, Workbench,
    WorkbenchCommand, WorkbenchEffect, WorkbenchTask,
};

impl Workbench {
    /// Returns whether this event is a plain Diff change-navigation key press.
    #[must_use]
    pub fn is_diff_change_navigation(&self, event: &Event) -> bool {
        self.active == Activity::Diff
            && self.modal.is_none()
            && !self.full_screen
            && !self.diff.captures_global_input()
            && matches!(
                event,
                Event::Key(key)
                    if key.kind == KeyEventKind::Press
                        && key.modifiers == KeyModifiers::NONE
                        && matches!(key.code, KeyCode::Char('n' | 'p'))
            )
    }

    pub fn filesystem_changed(&mut self) {
        self.explorer.filesystem_changed();
    }

    #[must_use]
    pub fn is_preparing(&self) -> bool {
        match self.active {
            super::Activity::Diff => self.diff.is_preparing(),
            super::Activity::Explorer => self.explorer.is_preparing(),
            super::Activity::History => self.history.is_preparing(),
        }
    }

    pub fn take_task(&mut self) -> Option<WorkbenchTask> {
        self.explorer.take_request().map(WorkbenchTask::Explorer)
    }
}

impl Tool for DiffActivity {
    fn handle_event(
        &mut self,
        event: &Event,
        area: Rect,
        _split: PaneSplit,
    ) -> Option<WorkbenchCommand> {
        self.renderer
            .map_event(event, &self.model, area)
            .map(|event| match event {
                RendererEvent::Consumed => WorkbenchCommand::Redraw,
                RendererEvent::Message(message) => WorkbenchCommand::Diff(message),
                RendererEvent::CopyPath { path, absolute } => {
                    WorkbenchCommand::Effect(WorkbenchEffect::CopyPath { path, absolute })
                }
            })
    }

    fn prepare_frame(&mut self, area: Rect, _split: PaneSplit) -> FramePreparation {
        let preparation = self.renderer.prepare_frame(&self.model, area);
        self.model.review.apply_preparation(&preparation);
        preparation
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _split: PaneSplit) {
        self.renderer.render_in(frame, &self.model, area);
    }

    fn is_preparing(&self) -> bool {
        self.renderer.is_preparing()
    }

    fn captures_global_input(&self) -> bool {
        self.renderer.has_open_picker_menu()
    }

    fn help_rows(&self) -> Vec<(String, &'static str)> {
        crate::diff::help_rows()
    }

    fn dismiss_popover(&mut self) {
        self.renderer.dismiss_picker_menus();
    }
}

impl Tool for ExplorerActivity {
    fn handle_event(
        &mut self,
        event: &Event,
        area: Rect,
        split: PaneSplit,
    ) -> Option<WorkbenchCommand> {
        ExplorerActivity::handle_event(self, event, area, split).map(|event| match event {
            ExplorerEvent::Consumed => WorkbenchCommand::Redraw,
            ExplorerEvent::CopyPath { path, absolute } => {
                WorkbenchCommand::Effect(WorkbenchEffect::CopyPath { path, absolute })
            }
        })
    }

    fn prepare_frame(&mut self, area: Rect, split: PaneSplit) -> FramePreparation {
        explorer_frame_preparation(self, area, split)
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, split: PaneSplit) {
        ExplorerActivity::render(self, frame, area, split);
    }

    fn is_preparing(&self) -> bool {
        ExplorerActivity::is_preparing(self)
    }

    fn captures_global_input(&self) -> bool {
        self.has_open_picker_menu()
    }

    fn commands(&self) -> &'static [Command] {
        ExplorerActivity::commands(self)
    }

    fn help_rows(&self) -> Vec<(String, &'static str)> {
        ExplorerActivity::help_rows(self)
    }

    fn dismiss_popover(&mut self) {
        self.dismiss_picker_menu();
    }

    fn execute_command(&mut self, command: CommandId) -> bool {
        ExplorerActivity::execute_command(self, command)
    }
}

impl Tool for HistoryActivity {
    fn handle_event(
        &mut self,
        event: &Event,
        area: Rect,
        split: PaneSplit,
    ) -> Option<WorkbenchCommand> {
        HistoryActivity::handle_event(self, event, area, split).map(|event| match event {
            HistoryEvent::Consumed => WorkbenchCommand::Redraw,
        })
    }

    fn prepare_frame(&mut self, area: Rect, split: PaneSplit) -> FramePreparation {
        HistoryActivity::prepare_frame(self, area, split)
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, split: PaneSplit) {
        HistoryActivity::render(self, frame, area, split);
    }

    fn is_preparing(&self) -> bool {
        HistoryActivity::is_preparing(self)
    }

    fn help_rows(&self) -> Vec<(String, &'static str)> {
        HistoryActivity::help_rows(self)
    }
}

pub(super) fn explorer_preparation(
    text_surface: TextSurfacePreparation,
    requested: Option<std::path::PathBuf>,
    displayed: Option<std::path::PathBuf>,
) -> FramePreparation {
    FramePreparation {
        content_revision: text_surface.document_revision,
        preparing: text_surface.mode == TextRenderMode::TextSkeleton,
        syntax_ready: text_surface.mode == TextRenderMode::Full,
        requested_explorer_file: requested,
        displayed_explorer_file: displayed,
        text_surface: Some(text_surface),
        ..FramePreparation::default()
    }
}

pub(super) fn explorer_frame_preparation(
    explorer: &mut ExplorerActivity,
    area: Rect,
    split: PaneSplit,
) -> FramePreparation {
    let text_surface = explorer.prepare_frame(area, split);
    let (requested, displayed) = explorer.document_paths();
    explorer_preparation(text_surface, requested, displayed)
}
