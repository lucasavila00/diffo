mod model;
mod view;
mod worker;

use std::{collections::VecDeque, path::PathBuf};

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_command::{Command, CommandId};
use diffo_core::RepositorySnapshot;
use diffo_text_view::{
    LINE_SCROLL_ROWS, ScrollCommand, ScrollbarAxis, Viewport, ViewportMetrics, WHEEL_SCROLL_ROWS,
    scrollbar_areas, scrollbar_axis_at, scrollbar_command,
};
use diffo_ui::PaneSplit;
use ratatui::{Frame, layout::Rect};

use model::ExplorerModel;
use view::{TreeAction, VIEWER_GUTTER_WIDTH, explorer_areas, tree_action_at, viewer_metrics};
pub use worker::{ExplorerOutcome, ExplorerRequest, ExplorerWorker};

pub const COLLAPSE_ALL_COMMAND: CommandId = CommandId::new("explorer.collapse_all");
pub const EXPAND_ALL_COMMAND: CommandId = CommandId::new("explorer.expand_all");

static COMMANDS: [Command; 2] = [
    Command {
        id: COLLAPSE_ALL_COMMAND,
        label: "Explorer: Collapse All Folders",
    },
    Command {
        id: EXPAND_ALL_COMMAND,
        label: "Explorer: Expand All Folders",
    },
];

pub struct ExplorerActivity {
    model: ExplorerModel,
    next_id: u64,
    latest_paths: u64,
    latest_file: u64,
    paths_pending: bool,
    queued: VecDeque<ExplorerRequest>,
    pending_path: Option<PathBuf>,
    viewport_rows: usize,
    viewport_columns: usize,
    maximum_horizontal_scroll: usize,
    scrollbar_drag: Option<ScrollbarAxis>,
}

impl ExplorerActivity {
    #[must_use]
    pub fn new(snapshot: RepositorySnapshot) -> Self {
        let mut activity = Self {
            model: ExplorerModel::new(snapshot),
            next_id: 0,
            latest_paths: 0,
            latest_file: 0,
            paths_pending: false,
            queued: VecDeque::new(),
            pending_path: None,
            viewport_rows: 1,
            viewport_columns: 1,
            maximum_horizontal_scroll: 0,
            scrollbar_drag: None,
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

    pub fn prepare_frame(&mut self, area: Rect, split: PaneSplit) {
        let areas = explorer_areas(area, split);
        let tree_rows = usize::from(areas.tree.height.saturating_sub(2));
        self.viewport_rows = usize::from(areas.viewer.height.saturating_sub(2)).max(1);
        self.viewport_columns = usize::from(
            areas
                .viewer
                .width
                .saturating_sub(2)
                .saturating_sub(VIEWER_GUTTER_WIDTH),
        );
        let viewer_inner = areas.viewer.inner(ratatui::layout::Margin {
            vertical: 1,
            horizontal: 1,
        });
        let metrics = self
            .model
            .viewer
            .as_ref()
            .map(|viewer| viewer_metrics(viewer_inner, &self.model, viewer));
        if let Some(metrics) = metrics {
            self.viewport_rows = metrics.viewport_rows.max(1);
            self.viewport_columns = metrics
                .viewport_columns
                .saturating_sub(usize::from(VIEWER_GUTTER_WIDTH));
        }
        let maximum_horizontal_scroll = metrics.map_or(0, |metrics| metrics.maximum_horizontal);
        self.maximum_horizontal_scroll = maximum_horizontal_scroll;
        self.model.viewer_horizontal_scroll = self
            .model
            .viewer_horizontal_scroll
            .min(maximum_horizontal_scroll);
        self.model.ensure_tree_selection_visible(tree_rows);
        let selected = self.model.selected_file().map(PathBuf::from);
        let displayed = self.model.viewer.as_ref().map(|viewer| &viewer.path);
        if selected.as_ref() != displayed && selected.as_ref() != self.pending_path.as_ref() {
            if let Some(path) = selected {
                self.request_file(path, 0);
            }
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, split: PaneSplit) {
        view::render(frame, area, split, &self.model, !self.viewer_syntax_ready());
    }

    #[must_use]
    pub fn commands(&self) -> &'static [Command] {
        &COMMANDS
    }

    pub fn execute_command(&mut self, command: CommandId) -> bool {
        if command == COLLAPSE_ALL_COMMAND {
            self.model.collapse_all();
        } else if command == EXPAND_ALL_COMMAND {
            self.model.expand_all();
        } else {
            return false;
        }
        self.selection_changed();
        true
    }

    pub fn handle_event(&mut self, event: &Event, area: Rect, split: PaneSplit) -> bool {
        if self.handle_viewer_mouse(event, area, split) {
            return true;
        }
        if let Event::Mouse(mouse) = event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left)
        {
            return self.handle_tree_click(area, split, mouse.column, mouse.row);
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
            KeyCode::Up => self.scroll_viewer(-LINE_SCROLL_ROWS),
            KeyCode::Down => self.scroll_viewer(LINE_SCROLL_ROWS),
            KeyCode::PageUp => {
                self.scroll_viewer(-i64::try_from(self.viewport_rows).unwrap_or(i64::MAX));
            }
            KeyCode::PageDown => {
                self.scroll_viewer(i64::try_from(self.viewport_rows).unwrap_or(i64::MAX));
            }
            KeyCode::Left => self.scroll_viewer_horizontal(-LINE_SCROLL_ROWS),
            KeyCode::Right => self.scroll_viewer_horizontal(LINE_SCROLL_ROWS),
            _ => return false,
        }
        true
    }

    fn handle_viewer_mouse(&mut self, event: &Event, area: Rect, split: PaneSplit) -> bool {
        let viewer_area = explorer_areas(area, split)
            .viewer
            .inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 1,
            });
        let viewer_metrics = self
            .model
            .viewer
            .as_ref()
            .map(|viewer| viewer_metrics(viewer_area, &self.model, viewer));
        if let Event::Mouse(mouse) = event {
            if mouse.kind == MouseEventKind::Up(MouseButton::Left) && self.scrollbar_drag.is_some()
            {
                self.scrollbar_drag = None;
                return true;
            }
            if let Some(metrics) = viewer_metrics {
                let scrollbar_areas = scrollbar_areas(viewer_area, metrics);
                let axis = if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    scrollbar_axis_at(scrollbar_areas, metrics, mouse.column, mouse.row)
                } else if mouse.kind == MouseEventKind::Drag(MouseButton::Left) {
                    self.scrollbar_drag
                } else {
                    None
                };
                if let Some(axis) = axis {
                    self.scrollbar_drag = Some(axis);
                    let command =
                        scrollbar_command(axis, scrollbar_areas, metrics, mouse.column, mouse.row);
                    self.apply_viewer_command(command, metrics);
                    return true;
                }
                if viewer_area.contains((mouse.column, mouse.row).into()) {
                    match mouse.kind {
                        MouseEventKind::ScrollUp => {
                            self.scroll_viewer(-WHEEL_SCROLL_ROWS);
                            return true;
                        }
                        MouseEventKind::ScrollDown => {
                            self.scroll_viewer(WHEEL_SCROLL_ROWS);
                            return true;
                        }
                        _ => {}
                    }
                }
            }
        }
        false
    }

    fn handle_tree_click(&mut self, area: Rect, split: PaneSplit, column: u16, row: u16) -> bool {
        let tree_area = explorer_areas(area, split).tree;
        if let Some(action) = tree_action_at(tree_area, column, row) {
            match action {
                TreeAction::CollapseAll => self.model.collapse_all(),
                TreeAction::ExpandAll => self.model.expand_all(),
            }
            self.selection_changed();
            return true;
        }
        let tree = tree_area.inner(ratatui::layout::Margin {
            vertical: 1,
            horizontal: 1,
        });
        if !tree.contains((column, row).into()) {
            return false;
        }
        let index = self
            .model
            .tree_scroll
            .saturating_add(usize::from(row.saturating_sub(tree.y)));
        self.model.select(index);
        if self
            .model
            .selected_entry()
            .is_some_and(|entry| entry.directory)
        {
            self.model.toggle_selected_directory();
        }
        self.selection_changed();
        true
    }

    fn selection_changed(&mut self) {
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
        let base = self.model.viewer_scroll;
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
        self.model.viewer_scroll = target;
        if !covered {
            self.request_file(viewer.path.clone(), target);
        }
    }

    fn scroll_viewer_horizontal(&mut self, amount: i64) {
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

    fn apply_viewer_command(&mut self, command: ScrollCommand, metrics: ViewportMetrics) {
        if let ScrollCommand::Vertical(target) = command {
            let current = self.model.viewer_scroll;
            let amount = i64::try_from(target).unwrap_or(i64::MAX)
                - i64::try_from(current).unwrap_or(i64::MAX);
            self.scroll_viewer(amount);
        } else {
            let mut viewport = Viewport {
                vertical: self.model.viewer_scroll,
                horizontal: self.model.viewer_horizontal_scroll,
            };
            viewport.apply(command, metrics);
            self.model.viewer_horizontal_scroll = viewport.horizontal;
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
                        let same_document = self
                            .model
                            .viewer
                            .as_ref()
                            .is_some_and(|displayed| displayed.path == viewer.path);
                        if !same_document {
                            self.model.viewer_scroll = 0;
                            self.model.viewer_horizontal_scroll = 0;
                        }
                        self.model.viewer = Some(viewer);
                        self.model.error = None;
                    }
                    Err(error) => self.model.error = Some(error),
                }
            }
            ExplorerOutcome::Paths { .. } | ExplorerOutcome::File { .. } => {}
        }
    }

    #[must_use]
    pub fn is_preparing(&self) -> bool {
        self.paths_pending || self.pending_path.is_some() || !self.queued.is_empty()
    }

    fn viewer_syntax_ready(&self) -> bool {
        let Some(viewer) = self.model.viewer.as_ref() else {
            return true;
        };
        if !viewer.syntax_eligible {
            return true;
        }
        let start = u32::try_from(self.model.viewer_scroll.saturating_add(1)).unwrap_or(u32::MAX);
        let end = u32::try_from(
            self.model
                .viewer_scroll
                .saturating_add(self.viewport_rows)
                .min(viewer.lines.len()),
        )
        .unwrap_or(u32::MAX);
        viewer
            .coverage
            .is_some_and(|coverage| coverage.start <= start && coverage.end >= end)
    }
}

#[cfg(test)]
mod tests {
    use super::model::Viewer;
    use super::*;
    use crossterm::event::MouseEvent;
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
        assert!(!explorer.handle_event(&event, Rect::default(), PaneSplit::default()));
    }

    #[test]
    fn clicking_a_directory_toggles_expansion() {
        let mut explorer = ExplorerActivity::new(RepositorySnapshot::default());
        explorer.accept(ExplorerOutcome::Paths {
            id: 1,
            result: Ok(vec![PathBuf::from("src/main.rs")]),
        });
        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        });

        assert!(explorer.handle_event(&click, Rect::new(0, 0, 100, 30), PaneSplit::default()));
        assert_eq!(explorer.model.visible.len(), 2);
        assert!(explorer.handle_event(&click, Rect::new(0, 0, 100, 30), PaneSplit::default()));
        assert_eq!(explorer.model.visible.len(), 1);
    }

    #[test]
    fn tree_header_buttons_expand_and_collapse_every_directory() {
        let mut explorer = ExplorerActivity::new(RepositorySnapshot::default());
        explorer.accept(ExplorerOutcome::Paths {
            id: 1,
            result: Ok(vec![PathBuf::from("src/nested/main.rs")]),
        });
        let area = Rect::new(0, 0, 100, 30);
        let split = PaneSplit::default();
        let tree = explorer_areas(area, split).tree;
        let click = |column| {
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row: tree.y,
                modifiers: KeyModifiers::NONE,
            })
        };

        assert!(explorer.handle_event(&click(tree.right() - 4), area, split));
        assert_eq!(explorer.model.visible.len(), 3);
        assert!(explorer.handle_event(&click(tree.right() - 8), area, split));
        assert_eq!(explorer.model.visible.len(), 1);
    }

    #[test]
    fn explorer_commands_use_the_same_state_transitions_as_header_buttons() {
        let mut explorer = ExplorerActivity::new(RepositorySnapshot::default());
        explorer.accept(ExplorerOutcome::Paths {
            id: 1,
            result: Ok(vec![PathBuf::from("src/nested/main.rs")]),
        });

        assert!(explorer.execute_command(EXPAND_ALL_COMMAND));
        assert_eq!(explorer.model.visible.len(), 3);
        assert!(explorer.execute_command(COLLAPSE_ALL_COMMAND));
        assert_eq!(explorer.model.visible.len(), 1);
        assert!(!explorer.execute_command(CommandId::new("unknown")));
    }

    #[test]
    fn horizontal_pan_clamps_to_the_visible_code_width_and_returns_to_zero() {
        let mut explorer = ExplorerActivity::new(RepositorySnapshot::default());
        explorer.model.viewer = Some(Viewer {
            path: PathBuf::from("wide.txt"),
            lines: vec!["x".repeat(100)],
            markers: HashMap::new(),
            highlighted: HashMap::new(),
            coverage: None,
            syntax_eligible: false,
            message: None,
        });
        let area = Rect::new(0, 0, 100, 30);
        explorer.prepare_frame(area, PaneSplit::default());
        let right = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Right,
            KeyModifiers::NONE,
        ));
        let left = Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Left,
            KeyModifiers::NONE,
        ));

        for _ in 0..100 {
            assert!(explorer.handle_event(&right, area, PaneSplit::default()));
        }
        assert_eq!(
            explorer.model.viewer_horizontal_scroll,
            100_usize.saturating_sub(explorer.viewport_columns)
        );
        for _ in 0..100 {
            assert!(explorer.handle_event(&left, area, PaneSplit::default()));
        }
        assert_eq!(explorer.model.viewer_horizontal_scroll, 0);
    }

    #[test]
    fn uncached_scroll_uses_the_model_viewport_until_coverage_arrives() {
        let mut explorer = ExplorerActivity::new(RepositorySnapshot::default());
        let path = PathBuf::from("large.rs");
        let lines = (1..=100)
            .map(|line| format!("let value_{line} = {line};"))
            .collect::<Vec<_>>();
        explorer.model.viewer = Some(Viewer {
            path: path.clone(),
            lines: lines.clone(),
            markers: HashMap::new(),
            highlighted: HashMap::new(),
            coverage: Some(diffo_highlight::LineRange { start: 1, end: 20 }),
            syntax_eligible: true,
            message: None,
        });
        explorer.viewport_rows = 10;

        explorer.scroll_viewer(40);
        assert_eq!(explorer.model.viewer_scroll, 40);
        assert!(!explorer.viewer_syntax_ready());
        let request_id = explorer.latest_file;

        explorer.accept(ExplorerOutcome::File {
            id: request_id,
            result: Ok(Viewer {
                path,
                lines,
                markers: HashMap::new(),
                highlighted: HashMap::new(),
                coverage: Some(diffo_highlight::LineRange { start: 41, end: 60 }),
                syntax_eligible: true,
                message: None,
            }),
        });

        assert_eq!(explorer.model.viewer_scroll, 40);
        assert!(explorer.viewer_syntax_ready());
    }
}
