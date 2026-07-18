mod model;
mod view;
mod worker;

use std::{collections::VecDeque, path::PathBuf};

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_command::{Command, CommandId};
use diffo_core::RepositorySnapshot;
use diffo_file_picker::{FilePicker, Outcome as PickerOutcome};
use diffo_text_view::{
    LINE_SCROLL_ROWS, ScrollCommand, ScrollbarAxis, TextRenderMode, TextSurface,
    TextSurfacePreparation, Viewport, ViewportMetrics, WHEEL_SCROLL_ROWS, scrollbar_areas,
    scrollbar_axis_at, scrollbar_command,
};
use diffo_ui::PaneSplit;
use ratatui::{Frame, layout::Rect};

use model::ExplorerModel;
use view::{VIEWER_GUTTER_WIDTH, explorer_areas, tree_document, viewer_metrics};
pub use worker::{ExplorerOutcome, ExplorerRequest, ExplorerWorker};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExplorerEvent {
    Consumed,
    CopyPath { path: PathBuf, absolute: bool },
}

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
    picker: FilePicker<PathBuf>,
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
    content_revision: u64,
}

impl ExplorerActivity {
    #[must_use]
    pub fn new(snapshot: RepositorySnapshot) -> Self {
        let mut activity = Self {
            model: ExplorerModel::new(snapshot),
            picker: FilePicker::default(),
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
            content_revision: 0,
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
        let status = self.model.entry(&path).and_then(|entry| entry.status);
        self.queued
            .retain(|request| !matches!(request, ExplorerRequest::File { .. }));
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
        if let Some(path) = self.selected_file().cloned() {
            self.request_file(path, self.model.viewer_scroll);
        }
    }

    pub fn prepare_frame(&mut self, area: Rect, split: PaneSplit) -> TextSurfacePreparation {
        let areas = explorer_areas(area, split);
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
        let selected_before = self.picker.selected().cloned();
        self.picker.prepare(
            areas.tree,
            tree_document(&self.model, split.border_style(), self.paths_pending),
            None,
        );
        if self.picker.selected() != selected_before.as_ref() {
            self.selection_changed();
        }
        let selected = self.selected_file().cloned();
        let displayed = self.model.viewer.as_ref().map(|viewer| viewer.path.clone());
        let text_missing = selected != displayed;
        if text_missing && selected.as_ref() != self.pending_path.as_ref() {
            if let Some(path) = selected.as_ref() {
                self.request_file(path.clone(), 0);
            }
        }
        let syntax_ready = self.viewer_syntax_ready();
        let coverage = self
            .model
            .viewer
            .as_ref()
            .and_then(|viewer| viewer.coverage.last().map(|range| (range.start, range.end)));
        TextSurfacePreparation {
            surface: TextSurface::Explorer,
            document_revision: self.content_revision,
            viewport: (self.model.viewer_scroll, self.viewport_rows),
            requested_range: (
                self.model.viewer_scroll,
                self.model.viewer_scroll.saturating_add(self.viewport_rows),
            ),
            mode: if text_missing {
                TextRenderMode::TextSkeleton
            } else if syntax_ready {
                TextRenderMode::Full
            } else {
                TextRenderMode::SyntaxSkeleton
            },
            coverage_before: coverage,
            coverage_after: coverage,
            request_id: self.pending_path.as_ref().map(|_| self.latest_file),
            cache_hit: !text_missing && syntax_ready,
            coalesced_request: false,
            stale_discarded: false,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, split: PaneSplit) {
        view::render(
            frame,
            area,
            split,
            &self.model,
            &self.picker,
            !self.viewer_syntax_ready(),
        );
    }

    #[must_use]
    pub fn commands(&self) -> &'static [Command] {
        &COMMANDS
    }

    pub fn execute_command(&mut self, command: CommandId) -> bool {
        if command == COLLAPSE_ALL_COMMAND {
            self.picker.collapse_all();
        } else if command == EXPAND_ALL_COMMAND {
            self.picker.expand_all();
        } else {
            return false;
        }
        self.selection_changed();
        true
    }

    pub fn handle_event(
        &mut self,
        event: &Event,
        area: Rect,
        split: PaneSplit,
    ) -> Option<ExplorerEvent> {
        let selected_before = self.picker.selected().cloned();
        if let Some(outcome) = self.picker.handle_event(event, area) {
            if self.picker.selected() != selected_before.as_ref() {
                self.selection_changed();
            }
            return match outcome {
                PickerOutcome::CopyPath { id, absolute } => {
                    Some(ExplorerEvent::CopyPath { path: id, absolute })
                }
                PickerOutcome::Consumed
                | PickerOutcome::Selected(_)
                | PickerOutcome::Activated(_)
                | PickerOutcome::RowAction(_)
                | PickerOutcome::PanelAction => Some(ExplorerEvent::Consumed),
            };
        }
        if self.handle_viewer_mouse(event, area, split) {
            return Some(ExplorerEvent::Consumed);
        }
        let Event::Key(key) = event else {
            return None;
        };
        if key.kind != KeyEventKind::Press || key.modifiers != KeyModifiers::NONE {
            return None;
        }
        match key.code {
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
            _ => return None,
        }
        Some(ExplorerEvent::Consumed)
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

    fn selection_changed(&mut self) {
        if let Some(path) = self.selected_file().cloned() {
            let displayed = self.model.viewer.as_ref().map(|viewer| &viewer.path);
            if displayed != Some(&path) && self.pending_path.as_ref() != Some(&path) {
                self.request_file(path, 0);
            }
        } else {
            self.pending_path = None;
        }
    }

    fn selected_file(&self) -> Option<&PathBuf> {
        self.picker
            .selected()
            .filter(|path| self.model.entry(path).is_some_and(|entry| !entry.directory))
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
            || viewer.coverage.iter().any(|range| {
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
                    Ok(mut viewer) => {
                        let same_document = self
                            .model
                            .viewer
                            .as_ref()
                            .is_some_and(|displayed| displayed.path == viewer.path);
                        if !same_document {
                            self.model.viewer_scroll = 0;
                            self.model.viewer_horizontal_scroll = 0;
                        }
                        if let Some(displayed) = self
                            .model
                            .viewer
                            .as_ref()
                            .filter(|displayed| displayed.path == viewer.path)
                        {
                            let mut highlighted = displayed.highlighted.clone();
                            highlighted.extend(std::mem::take(&mut viewer.highlighted));
                            viewer.highlighted = highlighted;
                            let incoming = std::mem::take(&mut viewer.coverage);
                            viewer.coverage.clone_from(&displayed.coverage);
                            viewer.coverage.extend(incoming);
                            merge_coverage(&mut viewer);
                        }
                        self.model.viewer = Some(viewer);
                        self.content_revision = self.content_revision.saturating_add(1);
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
            .iter()
            .any(|coverage| coverage.start <= start && coverage.end >= end)
    }
}

const MAX_COVERAGE_WINDOWS: usize = 8;

fn merge_coverage(viewer: &mut model::Viewer) {
    let mut merged = Vec::<diffo_highlight::LineRange>::new();
    for range in viewer.coverage.drain(..) {
        if let Some(existing) = merged.iter_mut().find(|existing| {
            existing.start <= range.end.saturating_add(1)
                && range.start <= existing.end.saturating_add(1)
        }) {
            existing.start = existing.start.min(range.start);
            existing.end = existing.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    if merged.len() > MAX_COVERAGE_WINDOWS {
        merged.drain(..merged.len() - MAX_COVERAGE_WINDOWS);
    }
    viewer
        .highlighted
        .retain(|line, _| merged.iter().any(|range| range.contains(*line)));
    viewer.coverage = merged;
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
                coverage: Vec::new(),
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
        assert!(
            explorer
                .handle_event(&event, Rect::default(), PaneSplit::default())
                .is_none()
        );
    }

    #[test]
    fn clicking_a_directory_toggles_expansion() {
        let mut explorer = ExplorerActivity::new(RepositorySnapshot::default());
        explorer.accept(ExplorerOutcome::Paths {
            id: 1,
            result: Ok(vec![PathBuf::from("src/main.rs")]),
        });
        let area = Rect::new(0, 0, 100, 30);
        explorer.prepare_frame(area, PaneSplit::default());
        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        });

        assert!(
            explorer
                .handle_event(&click, area, PaneSplit::default())
                .is_some()
        );
        assert_eq!(explorer.picker.visible_rows(), 2);
        assert!(
            explorer
                .handle_event(&click, area, PaneSplit::default())
                .is_some()
        );
        assert_eq!(explorer.picker.visible_rows(), 1);
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
        explorer.prepare_frame(area, split);
        let tree = explorer_areas(area, split).tree;
        let click = |column| {
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row: tree.y,
                modifiers: KeyModifiers::NONE,
            })
        };

        assert!(
            explorer
                .handle_event(&click(tree.right() - 4), area, split)
                .is_some()
        );
        assert_eq!(explorer.picker.visible_rows(), 3);
        assert!(
            explorer
                .handle_event(&click(tree.right() - 8), area, split)
                .is_some()
        );
        assert_eq!(explorer.picker.visible_rows(), 1);
    }

    #[test]
    fn explorer_commands_use_the_same_state_transitions_as_header_buttons() {
        let mut explorer = ExplorerActivity::new(RepositorySnapshot::default());
        explorer.accept(ExplorerOutcome::Paths {
            id: 1,
            result: Ok(vec![PathBuf::from("src/nested/main.rs")]),
        });
        explorer.prepare_frame(Rect::new(0, 0, 100, 30), PaneSplit::default());

        assert!(explorer.execute_command(EXPAND_ALL_COMMAND));
        assert_eq!(explorer.picker.visible_rows(), 3);
        assert!(explorer.execute_command(COLLAPSE_ALL_COMMAND));
        assert_eq!(explorer.picker.visible_rows(), 1);
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
            coverage: Vec::new(),
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
            assert!(
                explorer
                    .handle_event(&right, area, PaneSplit::default())
                    .is_some()
            );
        }
        assert_eq!(
            explorer.model.viewer_horizontal_scroll,
            100_usize.saturating_sub(explorer.viewport_columns)
        );
        for _ in 0..100 {
            assert!(
                explorer
                    .handle_event(&left, area, PaneSplit::default())
                    .is_some()
            );
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
            coverage: vec![diffo_highlight::LineRange { start: 1, end: 20 }],
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
                coverage: vec![diffo_highlight::LineRange { start: 41, end: 60 }],
                syntax_eligible: true,
                message: None,
            }),
        });

        assert_eq!(explorer.model.viewer_scroll, 40);
        assert!(explorer.viewer_syntax_ready());
        assert!(
            explorer
                .model
                .viewer
                .as_ref()
                .unwrap()
                .coverage
                .iter()
                .any(|range| range.contains(1))
        );

        explorer.scroll_viewer(-40);
        assert!(explorer.viewer_syntax_ready());
        assert!(explorer.pending_path.is_none());
    }

    #[test]
    fn file_requests_coalesce_to_the_newest_viewport() {
        let mut explorer = ExplorerActivity::new(RepositorySnapshot::default());
        explorer.queued.clear();

        explorer.request_file(PathBuf::from("large.rs"), 20);
        explorer.request_file(PathBuf::from("large.rs"), 80);

        let requests = explorer.queued.iter().collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert!(matches!(
            requests[0],
            ExplorerRequest::File { first_line: 80, .. }
        ));
    }
}
