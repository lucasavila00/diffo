use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_ui::{PaneSplit, design, icons, mouse_target_style, tool_areas};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Clear, Paragraph},
};

use super::{Activity, Workbench, WorkbenchCommand, explorer_preparation, workbench_areas};
use crate::diff::{FramePreparation, RendererEvent};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FullScreenAreas {
    pub(super) header: Rect,
    pub(super) buffer: Rect,
    pub(super) close: Rect,
}

#[must_use]
pub(super) fn areas(area: Rect) -> FullScreenAreas {
    let rows = Layout::vertical([
        Constraint::Length(design::SINGLE_LINE_HEIGHT),
        Constraint::Min(0),
    ])
    .split(area);
    let close = if rows[0].width == 0 {
        Rect::default()
    } else {
        Rect::new(
            rows[0].right().saturating_sub(1),
            rows[0].y,
            design::SINGLE_LINE_HEIGHT,
            rows[0].height,
        )
    };
    FullScreenAreas {
        header: rows[0],
        buffer: rows[1],
        close,
    }
}

pub(super) fn render_header(frame: &mut Frame, area: Rect, title: Line<'static>) {
    let areas = areas(area);
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(title), areas.header);
    frame.render_widget(Paragraph::new(icons::DISMISS), areas.close);
}

#[must_use]
pub(super) fn closes(event: &Event, area: Rect) -> bool {
    let Event::Mouse(mouse) = event else {
        return false;
    };
    mouse.kind == MouseEventKind::Up(MouseButton::Left)
        && areas(area).close.contains((mouse.column, mouse.row).into())
}

pub(super) fn is_toggle(event: &Event) -> bool {
    matches!(
        event,
        Event::Key(key)
            if key.kind == KeyEventKind::Press
                && key.code == KeyCode::Char('f')
                && key.modifiers == KeyModifiers::NONE
    )
}

#[must_use]
pub(super) fn entry_area(area: Rect, split: PaneSplit) -> Rect {
    let content = tool_areas(workbench_areas(area).content).content;
    let buffer = split.areas(content).trailing;
    let x = buffer.right().saturating_sub(design::INLINE_GAP);
    if buffer.height == 0 || x < buffer.x {
        return Rect::default();
    }
    Rect::new(
        x,
        buffer.y,
        design::SINGLE_LINE_HEIGHT,
        design::SINGLE_LINE_HEIGHT,
    )
}

fn opens(event: &Event, area: Rect, split: PaneSplit) -> bool {
    let Event::Mouse(mouse) = event else {
        return false;
    };
    mouse.kind == MouseEventKind::Up(MouseButton::Left)
        && entry_area(area, split).contains((mouse.column, mouse.row).into())
}

fn is_quit(event: &Event) -> bool {
    matches!(
        event,
        Event::Key(key)
            if key.kind == KeyEventKind::Press
                && (matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)))
    )
}

impl Workbench {
    fn full_screen_title(&self) -> Option<Line<'static>> {
        match self.active {
            Activity::Diff => self.diff.renderer.full_screen_title(),
            Activity::Explorer => self.explorer.full_screen_title(),
        }
    }

    pub(super) fn render_full_screen_entry(&self, frame: &mut Frame) {
        if self.full_screen_title().is_none() {
            return;
        }
        frame.render_widget(
            Paragraph::new(icons::MAXIMIZE).style(mouse_target_style()),
            entry_area(frame.area(), self.pane_split),
        );
    }

    pub(super) fn request_full_screen(&mut self, event: &Event, area: Rect) -> bool {
        if self.full_screen_title().is_none() {
            return false;
        }
        if is_toggle(event) {
            self.full_screen_pending = !self.full_screen_pending;
            return true;
        }
        if opens(event, area, self.pane_split) {
            self.full_screen_pending = true;
            return true;
        }
        false
    }

    pub(super) fn prepare_full_screen(&mut self, area: Rect) -> Option<FramePreparation> {
        if !self.full_screen && !self.full_screen_pending {
            return None;
        }
        if self.full_screen_title().is_none() {
            self.full_screen = false;
            self.full_screen_pending = false;
            return None;
        }
        let buffer = areas(area).buffer;
        let preparation = match self.active {
            Activity::Diff => {
                let preparation = self
                    .diff
                    .renderer
                    .prepare_full_screen(&self.diff.model, buffer);
                if let Some(viewport) = preparation.viewport_transition {
                    self.diff
                        .model
                        .set_diff_viewport(viewport.vertical, viewport.horizontal);
                }
                self.diff.model.clamp_diff_scroll(
                    preparation.maximum_vertical_scroll,
                    preparation.maximum_horizontal_scroll,
                );
                preparation
            }
            Activity::Explorer => {
                let text_surface = self.explorer.prepare_full_screen(buffer);
                let (requested, displayed) = self.explorer.document_paths();
                explorer_preparation(text_surface, requested, displayed)
            }
        };
        if self.full_screen_pending && !preparation.preparing && preparation.syntax_ready {
            self.full_screen = true;
            self.full_screen_pending = false;
        }
        Some(preparation)
    }

    pub(super) fn render_full_screen(&mut self, frame: &mut Frame) -> bool {
        if !self.full_screen {
            return false;
        }
        let area = frame.area();
        let Some(title) = self.full_screen_title() else {
            return false;
        };
        let buffer = areas(area).buffer;
        render_header(frame, area, title);
        match self.active {
            Activity::Diff => {
                self.diff
                    .renderer
                    .render_full_screen(frame, buffer, &self.diff.model);
            }
            Activity::Explorer => self.explorer.render_full_screen(frame, buffer),
        }
        true
    }

    pub(super) fn handle_full_screen_event(
        &mut self,
        event: &Event,
        area: Rect,
    ) -> Option<WorkbenchCommand> {
        if is_toggle(event) || closes(event, area) {
            self.full_screen = false;
            return Some(WorkbenchCommand::Redraw);
        }
        if is_quit(event) {
            self.should_quit = true;
            return None;
        }
        let buffer = areas(area).buffer;
        match self.active {
            Activity::Diff => self
                .diff
                .renderer
                .map_full_screen_event(event, &self.diff.model, buffer)
                .and_then(|event| match event {
                    RendererEvent::Message(message) => Some(WorkbenchCommand::Diff(message)),
                    RendererEvent::Consumed => Some(WorkbenchCommand::Redraw),
                    RendererEvent::CopyPath { .. } => None,
                }),
            Activity::Explorer => self
                .explorer
                .handle_full_screen_event(event, buffer)
                .map(|_| WorkbenchCommand::Redraw),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyModifiers, MouseEvent};

    #[test]
    fn reserves_one_header_row_and_one_close_cell() {
        let areas = areas(Rect::new(2, 3, 20, 8));

        assert_eq!(areas.header, Rect::new(2, 3, 20, 1));
        assert_eq!(areas.buffer, Rect::new(2, 4, 20, 7));
        assert_eq!(areas.close, Rect::new(21, 3, 1, 1));
    }

    #[test]
    fn only_left_release_on_x_closes() {
        let event = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: 21,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });

        assert!(closes(&event, Rect::new(2, 3, 20, 8)));
        assert!(!closes(&event, Rect::new(2, 4, 20, 8)));
    }

    #[test]
    fn entry_control_uses_the_normal_buffer_top_right_border() {
        assert_eq!(
            entry_area(Rect::new(0, 0, 100, 30), PaneSplit::default()),
            Rect::new(98, 0, 1, 1),
        );
    }
}
