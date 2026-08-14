use crate::diff::{
    CommandProgress, FramePreparation, render_command_progress, render_status, render_toasts,
};
use diffo_ui::text_view::{TextRenderMode, TextSurface};
use diffo_ui::{PaneSplit, command_progress_style, icons, mouse_target_style, tool_areas};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders, Paragraph},
};

use super::{
    Activity, Tool, Workbench, explorer_frame_preparation, render_activity_bar, workbench_areas,
};

pub(super) struct PresentationState {
    redraw: RedrawState,
    prepared: Option<PreparedPresentation>,
}

impl PresentationState {
    pub(super) const fn new() -> Self {
        Self {
            redraw: RedrawState::Requested,
            prepared: None,
        }
    }

    fn request(&mut self) {
        self.redraw = RedrawState::Requested;
    }

    fn take_request(&mut self) -> bool {
        std::mem::take(&mut self.redraw) == RedrawState::Requested
    }

    fn update(&mut self, preparation: &FramePreparation, viewport: Option<(usize, usize)>) {
        let prepared = PreparedPresentation::new(preparation, viewport);
        if self.prepared.as_ref() != Some(&prepared) {
            self.prepared = Some(prepared);
            self.request();
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum RedrawState {
    #[default]
    Clean,
    Requested,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PreparedPresentation {
    maximum_vertical_scroll: usize,
    maximum_horizontal_scroll: usize,
    content_revision: u64,
    syntax_ready: bool,
    displayed_file: Option<crate::diff::FileKey>,
    displayed_explorer_file: Option<std::path::PathBuf>,
    text_surface: Option<PreparedTextPresentation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PreparedTextPresentation {
    surface: TextSurface,
    document_revision: u64,
    viewport: (usize, usize),
    mode: TextRenderMode,
}

impl PreparedPresentation {
    fn new(preparation: &FramePreparation, viewport: Option<(usize, usize)>) -> Self {
        Self {
            maximum_vertical_scroll: preparation.maximum_vertical_scroll,
            maximum_horizontal_scroll: preparation.maximum_horizontal_scroll,
            content_revision: preparation.content_revision,
            syntax_ready: preparation.syntax_ready,
            displayed_file: preparation.displayed_file.clone(),
            displayed_explorer_file: preparation.displayed_explorer_file.clone(),
            text_surface: preparation.text_surface.as_ref().map(|surface| {
                PreparedTextPresentation {
                    surface: surface.surface,
                    document_revision: surface.document_revision,
                    viewport: viewport.unwrap_or(surface.viewport),
                    mode: surface.mode,
                }
            }),
        }
    }
}

impl Workbench {
    pub fn take_redraw_request(&mut self) -> bool {
        self.presentation.take_request()
    }

    pub(super) fn request_redraw(&mut self) {
        self.presentation.request();
    }

    pub fn prepare_frame(&mut self, area: Rect) -> FramePreparation {
        let preparation = if let Some(preparation) = self.prepare_full_screen(area) {
            preparation
        } else {
            let content = workbench_areas(area).content;
            self.sync_diff_pane_state();
            match self.active {
                Activity::Diff => self.diff.prepare_frame(content, self.pane_split),
                Activity::Explorer => {
                    explorer_frame_preparation(&mut self.explorer, content, self.pane_split)
                }
                Activity::History => {
                    Tool::prepare_frame(&mut self.history, content, self.pane_split)
                }
            }
        };
        let viewport = (self.active == Activity::Diff).then_some((
            self.diff.model.diff_scroll,
            self.diff.model.diff_horizontal_scroll,
        ));
        self.presentation.update(&preparation, viewport);
        preparation
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let content = workbench_areas(area).content;
        if self.render_full_screen(frame) {
            self.render_command_queue(frame, content);
            return;
        }
        match self.active {
            Activity::Diff => self.diff.render(frame, content, self.pane_split),
            Activity::Explorer => self.explorer.render(frame, content, self.pane_split),
            Activity::History => self.history.render(frame, content, self.pane_split),
        }
        render_status(frame, tool_areas(content).status, &self.diff.model);
        self.render_full_screen_entry(frame);
        render_pane_drag_marker(frame, tool_areas(content).content, self.pane_split);
        render_toasts(frame, self.toasts.as_slice(), content);
        render_activity_bar(frame, area, self.active);
        if self.command_progress.is_visible() {
            frame.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(command_progress_style(self.command_animation_tick)),
                area,
            );
        }
        self.render_modal(frame, content, area);
        self.render_command_queue(frame, content);
    }

    fn render_command_queue(&self, frame: &mut Frame, content: Rect) {
        let (rows, hidden) = self.command_progress_rows();
        if !rows.is_empty() {
            render_command_progress(
                frame,
                CommandProgress {
                    rows: &rows,
                    hidden,
                    animation_tick: self.command_animation_tick,
                },
                content,
            );
        }
    }
}

fn render_pane_drag_marker(frame: &mut Frame, area: Rect, split: PaneSplit) {
    let marker = split.seam_marker_area(area);
    if !marker.is_empty() {
        frame.render_widget(
            Paragraph::new(icons::PANE_DRAG).style(mouse_target_style()),
            marker,
        );
    }
}
