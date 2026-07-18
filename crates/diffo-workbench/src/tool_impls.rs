use super::{
    Clear, Command, CommandId, DiffActivity, Event, ExplorerActivity, ExplorerEvent, Frame,
    FramePreparation, PaneSplit, Rect, RendererEvent, SearchActivity, TextRenderMode,
    TextSurfacePreparation, Tool, WorkbenchCommand, WorkbenchEffect, tool_areas,
};

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
        self.model.commit_input_focused()
            || self.model.help_open
            || self.renderer.has_open_picker_menu()
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
}
