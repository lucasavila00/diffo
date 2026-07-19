use super::{
    Clear, Command, CommandId, DiffActivity, Event, ExplorerActivity, ExplorerEvent, Frame,
    FramePreparation, PaneSplit, Rect, RendererEvent, SearchActivity, TextRenderMode,
    TextSurfacePreparation, Tool, Workbench, WorkbenchCommand, WorkbenchEffect, WorkbenchTask,
    tool_areas,
};

impl Workbench {
    #[must_use]
    pub fn is_preparing(&self) -> bool {
        match self.active {
            super::Activity::Diff => self.diff.is_preparing(),
            super::Activity::Explorer => self.explorer.is_preparing(),
            super::Activity::Search => self.search.is_preparing(),
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
            .and_then(|event| match event {
                RendererEvent::Consumed => None,
                RendererEvent::Message(message) => Some(WorkbenchCommand::Diff(message)),
                RendererEvent::CopyPath { path, absolute } => {
                    Some(WorkbenchCommand::Effect(WorkbenchEffect::CopyPath {
                        path,
                        absolute,
                    }))
                }
            })
    }

    fn prepare_frame(&mut self, area: Rect, _split: PaneSplit) -> FramePreparation {
        let preparation = self.renderer.prepare_frame(&self.model, area);
        if let Some(viewport) = preparation.viewport_transition {
            self.model
                .set_diff_viewport(viewport.vertical, viewport.horizontal);
        }
        self.model.clamp_diff_scroll(
            preparation.maximum_vertical_scroll,
            preparation.maximum_horizontal_scroll,
        );
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
        ExplorerActivity::handle_event(self, event, area, split).and_then(|event| match event {
            ExplorerEvent::Consumed => None,
            ExplorerEvent::CopyPath { path, absolute } => {
                Some(WorkbenchCommand::Effect(WorkbenchEffect::CopyPath {
                    path,
                    absolute,
                }))
            }
        })
    }

    fn prepare_frame(&mut self, area: Rect, split: PaneSplit) -> FramePreparation {
        explorer_preparation(ExplorerActivity::prepare_frame(self, area, split))
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

pub(super) fn explorer_preparation(text_surface: TextSurfacePreparation) -> FramePreparation {
    FramePreparation {
        content_revision: text_surface.document_revision,
        preparing: text_surface.mode == TextRenderMode::TextSkeleton,
        syntax_ready: text_surface.mode == TextRenderMode::Full,
        text_surface: Some(text_surface),
        ..FramePreparation::default()
    }
}

impl Tool for SearchActivity {
    fn handle_event(
        &mut self,
        _event: &Event,
        _area: Rect,
        _split: PaneSplit,
    ) -> Option<WorkbenchCommand> {
        None
    }

    fn prepare_frame(&mut self, _area: Rect, _split: PaneSplit) -> FramePreparation {
        FramePreparation::default()
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, split: PaneSplit) {
        frame.render_widget(Clear, area);
        let content = tool_areas(area).content;
        let panes = split.areas(content);
        frame.render_widget(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(split.border_style()),
            panes.leading,
        );
        frame.render_widget(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(split.border_style()),
            panes.trailing,
        );
    }

    fn is_preparing(&self) -> bool {
        false
    }

    fn help_rows(&self) -> Vec<(String, &'static str)> {
        vec![("q / Esc / Ctrl+c".to_owned(), "Quit")]
    }
}
