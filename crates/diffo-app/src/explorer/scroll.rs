use diffo_ui::{
    maximum_scroll,
    text_view::{ScrollCommand, Viewport, ViewportMetrics},
};

use super::ExplorerActivity;

impl ExplorerActivity {
    pub(super) fn prepare_viewer_scroll(&mut self) {
        let Some(requested) = self.vertical_scroll.requested() else {
            return;
        };
        let Some(viewer) = self.model.viewer.as_ref() else {
            self.vertical_scroll.clear();
            return;
        };
        let path = viewer.path.clone();
        let target = self
            .vertical_scroll
            .request(
                ScrollCommand::Vertical(requested),
                self.model.viewer_scroll,
                ViewportMetrics {
                    maximum_vertical: maximum_scroll(viewer.lines.len(), self.viewport_rows),
                    ..ViewportMetrics::default()
                },
            )
            .unwrap_or(self.model.viewer_scroll);
        if self.viewer_syntax_ready_at(target) {
            self.model.viewer_scroll = self.vertical_scroll.take_ready(true).unwrap_or(target);
        } else if self.pending_path.as_ref() != Some(&path) {
            self.request_file(path, target, false);
        }
    }

    pub(super) fn scroll_viewer(&mut self, amount: i64) {
        self.request_viewer_scroll(ScrollCommand::Lines(amount));
    }

    fn request_viewer_scroll(&mut self, command: ScrollCommand) {
        let Some(viewer) = self.model.viewer.as_ref() else {
            return;
        };
        let target = self
            .vertical_scroll
            .request(
                command,
                self.model.viewer_scroll,
                ViewportMetrics {
                    maximum_vertical: maximum_scroll(viewer.lines.len(), self.viewport_rows),
                    ..ViewportMetrics::default()
                },
            )
            .unwrap_or(self.model.viewer_scroll);
        if self.viewer_syntax_ready_at(target) {
            self.model.viewer_scroll = self.vertical_scroll.take_ready(true).unwrap_or(target);
        } else {
            self.request_file(viewer.path.clone(), target, false);
        }
    }

    pub(super) fn scroll_viewer_horizontal(&mut self, amount: i64) {
        let mut viewport = Viewport {
            vertical: self.model.viewer_scroll,
            horizontal: self.model.viewer_horizontal_scroll,
        };
        viewport.apply(
            ScrollCommand::Columns(amount),
            ViewportMetrics {
                maximum_horizontal: self.maximum_horizontal_scroll,
                ..ViewportMetrics::default()
            },
        );
        self.model.viewer_horizontal_scroll = viewport.horizontal;
    }

    pub(super) fn apply_viewer_command(
        &mut self,
        command: ScrollCommand,
        metrics: ViewportMetrics,
    ) {
        if matches!(
            command,
            ScrollCommand::Lines(_)
                | ScrollCommand::Page(_)
                | ScrollCommand::Vertical(_)
                | ScrollCommand::Home
                | ScrollCommand::End
        ) {
            self.request_viewer_scroll(command);
        } else {
            let mut viewport = Viewport {
                vertical: self.model.viewer_scroll,
                horizontal: self.model.viewer_horizontal_scroll,
            };
            viewport.apply(command, metrics);
            self.model.viewer_horizontal_scroll = viewport.horizontal;
        }
    }
}
