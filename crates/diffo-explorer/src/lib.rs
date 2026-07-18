mod model;
mod view;
mod worker;

use std::{collections::VecDeque, path::PathBuf};

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_core::RepositorySnapshot;
use ratatui::{Frame, layout::Rect};

use model::ExplorerModel;
use view::explorer_areas;
pub use worker::{ExplorerOutcome, ExplorerRequest, ExplorerWorker};

pub struct ExplorerActivity {
    model: ExplorerModel,
    next_id: u64,
    latest_paths: u64,
    latest_file: u64,
    paths_pending: bool,
    queued: VecDeque<ExplorerRequest>,
    pending_path: Option<PathBuf>,
    pending_scroll: Option<usize>,
    viewport_rows: usize,
}

impl ExplorerActivity {
    pub fn new(snapshot: RepositorySnapshot) -> Self {
        let mut activity = Self {
            model: ExplorerModel::new(snapshot),
            next_id: 0,
            latest_paths: 0,
            latest_file: 0,
            paths_pending: false,
            queued: VecDeque::new(),
            pending_path: None,
            pending_scroll: None,
            viewport_rows: 1,
        };
        activity.request_paths();
        activity
    }

    fn next_id(&mut self) -> u64 {
        self.next_id = self.next_id.saturating_add(1);
        self.next_id
    }

    fn request_paths(&mut self) {
        let id = self.next_id();
        self.latest_paths = id;
        self.paths_pending = true;
        self.queued.push_back(ExplorerRequest::Paths { id });
    }

    fn request_file(&mut self, path: PathBuf, first_line: usize) {
        let id = self.next_id();
        self.latest_file = id;
        self.pending_path = Some(path.clone());
        let status = self
            .model
            .selected_entry()
            .filter(|entry| entry.path == path)
            .and_then(|entry| entry.status);
        self.queued.push_back(ExplorerRequest::File {
            id,
            path,
            status,
            first_line,
            viewport_rows: self.viewport_rows,
        });
    }

    pub fn repository_changed(&mut self, snapshot: RepositorySnapshot) {
        if !self.model.repository_changed(snapshot) {
            return;
        }
        self.request_paths();
        if let Some(path) = self.model.selected_file().map(PathBuf::from) {
            self.request_file(path, self.model.viewer_scroll);
        }
    }

    pub fn prepare_frame(&mut self, area: Rect) {
        let areas = explorer_areas(area);
        let tree_rows = usize::from(areas.tree.height.saturating_sub(2));
        self.viewport_rows = usize::from(areas.viewer.height.saturating_sub(2)).max(1);
        self.model.ensure_tree_selection_visible(tree_rows);
        let selected = self.model.selected_file().map(PathBuf::from);
        let displayed = self.model.viewer.as_ref().map(|viewer| &viewer.path);
        if selected.as_ref() != displayed && selected.as_ref() != self.pending_path.as_ref() {
            if let Some(path) = selected {
                self.request_file(path, 0);
            }
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        view::render(frame, area, &self.model);
    }

    pub fn handle_event(&mut self, event: &Event, area: Rect) -> bool {
        if let Event::Mouse(mouse) = event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left)
        {
            let tree = explorer_areas(area).tree.inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 1,
            });
            if tree.contains((mouse.column, mouse.row).into()) {
                let index = self
                    .model
                    .tree_scroll
                    .saturating_add(usize::from(mouse.row.saturating_sub(tree.y)));
                self.model.select(index);
                self.selection_changed();
                return true;
            }
        }
        let Event::Key(key) = event else {
            return false;
        };
        if key.kind != KeyEventKind::Press || key.modifiers != KeyModifiers::NONE {
            return false;
        }
        match key.code {
            KeyCode::Char('j') => {
                self.model.select_by(1);
                self.selection_changed();
            }
            KeyCode::Char('k') => {
                self.model.select_by(-1);
                self.selection_changed();
            }
            KeyCode::Enter => {
                self.model.toggle_selected_directory();
                self.selection_changed();
            }
            KeyCode::Up => self.scroll_viewer(-4),
            KeyCode::Down => self.scroll_viewer(4),
            KeyCode::PageUp => {
                self.scroll_viewer(-i64::try_from(self.viewport_rows).unwrap_or(i64::MAX));
            }
            KeyCode::PageDown => {
                self.scroll_viewer(i64::try_from(self.viewport_rows).unwrap_or(i64::MAX));
            }
            KeyCode::Left => {
                self.model.viewer_horizontal_scroll =
                    self.model.viewer_horizontal_scroll.saturating_sub(4);
            }
            KeyCode::Right => {
                let maximum = self
                    .model
                    .viewer
                    .as_ref()
                    .map_or(0, |viewer| viewer.maximum_width);
                self.model.viewer_horizontal_scroll = self
                    .model
                    .viewer_horizontal_scroll
                    .saturating_add(4)
                    .min(maximum);
            }
            _ => return false,
        }
        true
    }

    fn selection_changed(&mut self) {
        self.pending_scroll = None;
        if let Some(path) = self.model.selected_file().map(PathBuf::from) {
            self.request_file(path, 0);
        } else {
            self.pending_path = None;
        }
    }

    fn scroll_viewer(&mut self, amount: i64) {
        let Some(viewer) = self.model.viewer.as_ref() else {
            return;
        };
        let base = self.pending_scroll.unwrap_or(self.model.viewer_scroll);
        let magnitude = usize::try_from(amount.unsigned_abs()).unwrap_or(usize::MAX);
        let target = if amount < 0 {
            base.saturating_sub(magnitude)
        } else {
            base.saturating_add(magnitude)
                .min(viewer.lines.len().saturating_sub(self.viewport_rows))
        };
        let visible_end = target.saturating_add(self.viewport_rows);
        let covered = !viewer.syntax_eligible
            || viewer.coverage.is_some_and(|range| {
                let start = u32::try_from(target.saturating_add(1)).unwrap_or(u32::MAX);
                let end = u32::try_from(visible_end.min(viewer.lines.len())).unwrap_or(u32::MAX);
                range.start <= start && range.end >= end
            });
        if covered && self.pending_scroll.is_none() {
            self.model.viewer_scroll = target;
        } else if self.pending_scroll != Some(target) {
            self.pending_scroll = Some(target);
            self.request_file(viewer.path.clone(), target);
        }
    }

    pub fn take_request(&mut self) -> Option<ExplorerRequest> {
        self.queued.pop_front()
    }

    pub fn accept(&mut self, outcome: ExplorerOutcome) {
        match outcome {
            ExplorerOutcome::Paths { id, result } if id == self.latest_paths => match result {
                Ok(paths) => {
                    self.paths_pending = false;
                    self.model.error = None;
                    self.model.install_paths(paths);
                }
                Err(error) => {
                    self.paths_pending = false;
                    self.model.error = Some(error);
                }
            },
            ExplorerOutcome::File { id, result } if id == self.latest_file => {
                self.pending_path = None;
                match result {
                    Ok(viewer) => {
                        let requested_scroll = self.pending_scroll.take().unwrap_or(0);
                        self.model.viewer_scroll = requested_scroll;
                        self.model.viewer_horizontal_scroll = 0;
                        self.model.viewer = Some(viewer);
                        self.model.error = None;
                    }
                    Err(error) => self.model.error = Some(error),
                }
            }
            ExplorerOutcome::Paths { .. } | ExplorerOutcome::File { .. } => {}
        }
    }

    pub fn is_preparing(&self) -> bool {
        self.paths_pending || self.pending_path.is_some() || !self.queued.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::model::Viewer;
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn stale_file_results_do_not_commit() {
        let mut explorer = ExplorerActivity::new(RepositorySnapshot::default());
        explorer.latest_file = 2;
        explorer.pending_path = Some(PathBuf::from("new.rs"));
        explorer.accept(ExplorerOutcome::File {
            id: 1,
            result: Ok(Viewer {
                path: PathBuf::from("old.rs"),
                lines: vec!["old".to_owned()],
                markers: HashMap::new(),
                highlighted: HashMap::new(),
                coverage: None,
                syntax_eligible: false,
                message: None,
                maximum_width: 3,
            }),
        });
        assert!(explorer.model.viewer.is_none());
        assert_eq!(explorer.pending_path, Some(PathBuf::from("new.rs")));
    }

    #[test]
    fn uppercase_shortcuts_are_rejected() {
        let mut explorer = ExplorerActivity::new(RepositorySnapshot::default());
        let event = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Char('J'),
            KeyModifiers::SHIFT,
        ));
        assert!(!explorer.handle_event(&event, Rect::default()));
    }
}
