//! Repository tree, file viewer, input routing, and file preparation.

mod model;
mod quick_open;
mod scroll;
mod view;
mod worker;

use std::{collections::VecDeque, path::PathBuf};

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_core::RepositorySnapshot;
use diffo_ui::command_palette::{Command, CommandId};
use diffo_ui::file_picker::{FilePicker, Outcome as PickerOutcome};
use diffo_ui::text_view::{
    LINE_SCROLL_ROWS, PreparedVerticalScroll, ScrollCommand, ScrollbarAxis, TextRenderMode,
    TextSurface, TextSurfacePreparation, scrollbar_areas, scrollbar_axis_at, scrollbar_command,
    syntax_prefetch_viewports, wheel_scroll_command,
};
use diffo_ui::{PaneSplit, design};
use ratatui::{Frame, layout::Rect, text::Line};

pub use model::ExplorerDocumentId;
use model::{EntryId, ExplorerModel};
use view::{
    VIEWER_GUTTER_WIDTH, entry_label, explorer_areas, full_screen_viewer_metrics, tree_document,
    viewer_metrics,
};
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
    picker: FilePicker<EntryId>,
    next_id: u64,
    latest_paths: u64,
    latest_quick_open_paths: u64,
    latest_load: u64,
    latest_window: u64,
    paths_pending: bool,
    quick_open_paths_pending: bool,
    quick_open_paths: Vec<PathBuf>,
    queued: VecDeque<ExplorerRequest>,
    pending_path: Option<PathBuf>,
    pending_window: Option<(u64, ExplorerDocumentId)>,
    vertical_scroll: PreparedVerticalScroll,
    pending_quick_open: Option<PathBuf>,
    viewport_rows: usize,
    viewport_columns: usize,
    maximum_horizontal_scroll: usize,
    scrollbar_drag: Option<ScrollbarAxis>,
    content_revision: u64,
}

impl ExplorerActivity {
    #[must_use]
    pub fn new(snapshot: &RepositorySnapshot) -> Self {
        let mut activity = Self {
            model: ExplorerModel::new(snapshot),
            picker: FilePicker::default(),
            next_id: 0,
            latest_paths: 0,
            latest_quick_open_paths: 0,
            latest_load: 0,
            latest_window: 0,
            paths_pending: false,
            quick_open_paths_pending: false,
            quick_open_paths: Vec::new(),
            queued: VecDeque::new(),
            pending_path: None,
            pending_window: None,
            vertical_scroll: PreparedVerticalScroll::default(),
            pending_quick_open: None,
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
        self.queued
            .retain(|request| !matches!(request, ExplorerRequest::Paths { .. }));
        self.queued.push_back(ExplorerRequest::Paths { id });
    }

    pub(crate) fn request_quick_open_paths(&mut self) {
        let id = self.next_id();
        self.latest_quick_open_paths = id;
        self.quick_open_paths_pending = true;
        self.quick_open_paths.clear();
        self.queued
            .retain(|request| !matches!(request, ExplorerRequest::QuickOpenPaths { .. }));
        self.queued
            .push_back(ExplorerRequest::QuickOpenPaths { id });
    }

    fn request_file_load(&mut self, path: PathBuf, first_line: usize) {
        let Some((status, title)) = self
            .model
            .file_entry(&path)
            .map(|entry| (entry.status, entry_label(entry)))
        else {
            return;
        };
        let id = self.next_id();
        let committed = self
            .model
            .viewer
            .as_ref()
            .filter(|viewer| viewer.path == path)
            .map_or(first_line, |_| self.model.viewer_scroll);
        self.latest_load = id;
        self.pending_path = Some(path.clone());
        self.pending_window = None;
        self.queued.retain(|request| {
            !matches!(
                request,
                ExplorerRequest::LoadFile { .. } | ExplorerRequest::HighlightWindow { .. }
            )
        });
        self.queued.push_back(ExplorerRequest::LoadFile {
            id,
            path,
            title,
            status,
            first_line,
            viewport_rows: self.viewport_rows,
            window_viewports: syntax_prefetch_viewports(committed, first_line, self.viewport_rows),
        });
    }

    fn request_syntax_window(&mut self, first_line: usize) {
        let Some(viewer) = self.model.viewer.as_ref() else {
            return;
        };
        let document_id = viewer.document_id;
        let path = viewer.path.clone();
        let lines = viewer.lines.clone();
        let id = self.next_id();
        self.latest_window = id;
        self.pending_window = Some((id, document_id));
        self.queued
            .retain(|request| !matches!(request, ExplorerRequest::HighlightWindow { .. }));
        self.queued.push_back(ExplorerRequest::HighlightWindow {
            id,
            document_id,
            path,
            lines,
            first_line,
            viewport_rows: self.viewport_rows,
            window_viewports: syntax_prefetch_viewports(
                self.model.viewer_scroll,
                first_line,
                self.viewport_rows,
            ),
        });
    }

    pub fn repository_changed(&mut self, snapshot: &RepositorySnapshot) {
        if !self.model.repository_changed(snapshot) {
            return;
        }
        self.vertical_scroll.clear();
        self.request_paths();
        if let Some(path) = self.selected_file().cloned() {
            self.request_file_load(path, self.model.viewer_scroll);
        }
    }

    pub fn filesystem_changed(&mut self) {
        self.vertical_scroll.clear();
        self.request_paths();
        if let Some(path) = self.selected_file().cloned() {
            self.request_file_load(path, self.model.viewer_scroll);
        }
    }

    pub fn prepare_frame(&mut self, area: Rect, split: PaneSplit) -> TextSurfacePreparation {
        let areas = explorer_areas(area, split);
        self.viewport_rows = usize::from(design::panel_content_extent(areas.viewer.height))
            .max(usize::from(design::SINGLE_LINE_HEIGHT));
        self.viewport_columns = usize::from(
            design::panel_content_extent(areas.viewer.width).saturating_sub(VIEWER_GUTTER_WIDTH),
        );
        let viewer_inner = areas.viewer.inner(design::PANEL_INSET);
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
        self.prepare_viewer_scroll();
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
        if text_missing
            && selected.as_ref() != self.pending_path.as_ref()
            && let Some(path) = selected.as_ref()
        {
            self.request_file_load(path.clone(), 0);
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
            request_id: self.pending_request_id(),
            cache_hit: !text_missing && syntax_ready,
            coalesced_request: false,
            stale_discarded: false,
        }
    }

    pub fn prepare_full_screen(&mut self, area: Rect) -> TextSurfacePreparation {
        self.scrollbar_drag = None;
        let metrics = self
            .model
            .viewer
            .as_ref()
            .map(|viewer| full_screen_viewer_metrics(area, &self.model, viewer));
        if let Some(metrics) = metrics {
            self.viewport_rows = metrics.viewport_rows.max(1);
            self.viewport_columns = metrics.viewport_columns;
            self.maximum_horizontal_scroll = metrics.maximum_horizontal;
            self.model.viewer_scroll = self.model.viewer_scroll.min(metrics.maximum_vertical);
            self.model.viewer_horizontal_scroll = self
                .model
                .viewer_horizontal_scroll
                .min(metrics.maximum_horizontal);
        }
        self.prepare_viewer_scroll();
        let selected = self.selected_file().cloned();
        let displayed = self.model.viewer.as_ref().map(|viewer| viewer.path.clone());
        let text_missing = selected != displayed;
        if text_missing && selected.as_ref() != self.pending_path.as_ref() {
            if let Some(path) = selected.as_ref() {
                self.request_file_load(path.clone(), 0);
            }
        } else if !self.viewer_syntax_ready() && self.pending_window.is_none() {
            self.request_syntax_window(self.model.viewer_scroll);
        }
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
            } else if self.viewer_syntax_ready() {
                TextRenderMode::Full
            } else {
                TextRenderMode::SyntaxSkeleton
            },
            coverage_before: coverage,
            coverage_after: coverage,
            request_id: self.pending_request_id(),
            cache_hit: !text_missing && self.viewer_syntax_ready(),
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

    pub fn render_full_screen(&self, frame: &mut Frame, area: Rect) {
        view::render_full_screen(frame, area, &self.model, !self.viewer_syntax_ready());
    }

    #[must_use]
    pub fn full_screen_title(&self) -> Option<Line<'static>> {
        self.model
            .viewer
            .as_ref()
            .map(|viewer| viewer.title.as_ref().clone())
    }

    #[must_use]
    pub fn has_open_picker_menu(&self) -> bool {
        self.picker.has_open_menu()
    }

    pub fn dismiss_picker_menu(&mut self) {
        self.picker.dismiss_menu();
    }

    #[must_use]
    pub fn commands(&self) -> &'static [Command] {
        &COMMANDS
    }

    #[must_use]
    pub fn help_rows(&self) -> Vec<(String, &'static str)> {
        diffo_ui::file_picker::help_rows()
            .into_iter()
            .chain([
                ("Enter".to_owned(), "Expand / collapse selected folder"),
                ("f".to_owned(), "Toggle full-screen viewer"),
                ("q / Esc / Ctrl+c".to_owned(), "Quit"),
                ("↑".to_owned(), "Scroll viewer up by four lines"),
                ("↓".to_owned(), "Scroll viewer down by four lines"),
                ("Page Up".to_owned(), "Scroll viewer up one page"),
                ("Page Down".to_owned(), "Scroll viewer down one page"),
                ("←".to_owned(), "Scroll viewer left by four columns"),
                ("→".to_owned(), "Scroll viewer right by four columns"),
            ])
            .collect()
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
        let menu_before = self.picker.has_open_menu();
        if let Some(outcome) = self.picker.handle_event(event, area) {
            if self.picker.selected() != selected_before.as_ref() {
                self.selection_changed();
            }
            return match outcome {
                PickerOutcome::CopyPath {
                    id: EntryId::File(path) | EntryId::Directory(path),
                    absolute,
                } => Some(ExplorerEvent::CopyPath { path, absolute }),
                PickerOutcome::Selected(id @ EntryId::File(_))
                    if selected_before.as_ref() == Some(&id)
                        && self.picker.has_open_menu() == menu_before =>
                {
                    None
                }
                PickerOutcome::Consumed
                | PickerOutcome::Selected(_)
                | PickerOutcome::Activated(_)
                | PickerOutcome::RowAction(_)
                | PickerOutcome::DestructiveAction(_)
                | PickerOutcome::PanelAction => Some(ExplorerEvent::Consumed),
            };
        }
        if self.picker.has_open_menu() {
            return Some(ExplorerEvent::Consumed);
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
        let viewport_before = (
            self.model.viewer_scroll,
            self.model.viewer_horizontal_scroll,
        );
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
        (viewport_before
            != (
                self.model.viewer_scroll,
                self.model.viewer_horizontal_scroll,
            ))
            .then_some(ExplorerEvent::Consumed)
    }

    pub fn handle_full_screen_event(&mut self, event: &Event, area: Rect) -> Option<ExplorerEvent> {
        let viewport_before = (
            self.model.viewer_scroll,
            self.model.viewer_horizontal_scroll,
        );
        let command = match event {
            Event::Key(key)
                if key.kind == KeyEventKind::Press && key.modifiers == KeyModifiers::NONE =>
            {
                match key.code {
                    KeyCode::Up => ScrollCommand::Lines(-LINE_SCROLL_ROWS),
                    KeyCode::Down => ScrollCommand::Lines(LINE_SCROLL_ROWS),
                    KeyCode::PageUp => {
                        ScrollCommand::Lines(-i64::try_from(self.viewport_rows).unwrap_or(i64::MAX))
                    }
                    KeyCode::PageDown => {
                        ScrollCommand::Lines(i64::try_from(self.viewport_rows).unwrap_or(i64::MAX))
                    }
                    KeyCode::Left => ScrollCommand::Columns(-LINE_SCROLL_ROWS),
                    KeyCode::Right => ScrollCommand::Columns(LINE_SCROLL_ROWS),
                    _ => return None,
                }
            }
            Event::Mouse(mouse) if area.contains((mouse.column, mouse.row).into()) => {
                wheel_scroll_command(mouse.kind)?
            }
            _ => return None,
        };
        self.apply_full_screen_viewer_command(command);
        (viewport_before
            != (
                self.model.viewer_scroll,
                self.model.viewer_horizontal_scroll,
            ))
            .then_some(ExplorerEvent::Consumed)
    }

    fn handle_viewer_mouse(&mut self, event: &Event, area: Rect, split: PaneSplit) -> bool {
        let viewer_area = explorer_areas(area, split)
            .viewer
            .inner(design::PANEL_INSET);
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
                if viewer_area.contains((mouse.column, mouse.row).into())
                    && let Some(command) = wheel_scroll_command(mouse.kind)
                {
                    self.apply_viewer_command(command, metrics);
                    return true;
                }
            }
        }
        false
    }

    fn selection_changed(&mut self) {
        self.vertical_scroll.clear();
        if let Some(path) = self.selected_file().cloned() {
            let displayed = self.model.viewer.as_ref().map(|viewer| &viewer.path);
            if displayed != Some(&path) && self.pending_path.as_ref() != Some(&path) {
                self.request_file_load(path, 0);
            }
        } else {
            self.pending_path = None;
            self.pending_window = None;
        }
    }

    fn selected_file(&self) -> Option<&PathBuf> {
        match self.picker.selected() {
            Some(EntryId::File(path)) => Some(path),
            Some(EntryId::Directory(_)) | None => None,
        }
    }

    pub(crate) fn document_paths(&self) -> (Option<PathBuf>, Option<PathBuf>) {
        (
            self.selected_file().cloned(),
            self.model.viewer.as_ref().map(|viewer| viewer.path.clone()),
        )
    }

    pub fn take_request(&mut self) -> Option<ExplorerRequest> {
        self.queued.pop_front()
    }

    pub fn accept(&mut self, outcome: ExplorerOutcome) -> (Option<(String, String)>, bool) {
        match outcome {
            ExplorerOutcome::Paths { id, result } if id == self.latest_paths => match result {
                Ok(paths) => {
                    self.paths_pending = false;
                    let changed = self.model.install_paths(paths);
                    (None, changed)
                }
                Err(error) => {
                    self.paths_pending = false;
                    (Some(("Explorer refresh failed".to_owned(), error)), true)
                }
            },
            ExplorerOutcome::QuickOpenPaths { id, result }
                if id == self.latest_quick_open_paths =>
            {
                self.quick_open_paths_pending = false;
                match result {
                    Ok(mut paths) => {
                        paths.sort();
                        paths.dedup();
                        let changed = self.quick_open_paths != paths;
                        self.quick_open_paths = paths;
                        (None, changed)
                    }
                    Err(error) => (Some(("Quick Open refresh failed".to_owned(), error)), true),
                }
            }
            ExplorerOutcome::FileLoaded { id, result } if id == self.latest_load => {
                let requested_path = self.pending_path.take();
                match result {
                    Ok(viewer) => {
                        if self.pending_quick_open.as_ref() == Some(&viewer.path)
                            && self.model.file_entry(&viewer.path).is_none()
                        {
                            self.pending_quick_open = None;
                            self.request_paths();
                            return (None, true);
                        }
                        let same_document = self
                            .model
                            .viewer
                            .as_ref()
                            .is_some_and(|displayed| displayed.path == viewer.path);
                        if !same_document {
                            self.model.viewer_scroll = 0;
                            self.model.viewer_horizontal_scroll = 0;
                            self.vertical_scroll.clear();
                        }
                        let viewer_changed = self.model.viewer.as_ref() != Some(&viewer);
                        self.pending_window = None;
                        self.model.viewer = Some(viewer);
                        if let Some(path) = self.pending_quick_open.take() {
                            self.commit_quick_open_selection(&path);
                        }
                        if viewer_changed {
                            self.content_revision = self.content_revision.saturating_add(1);
                        }
                        (None, viewer_changed)
                    }
                    Err(error) => {
                        self.vertical_scroll.clear();
                        if self.pending_quick_open.take().is_some() {
                            self.request_paths();
                            (None, true)
                        } else if requested_path
                            .as_ref()
                            .is_some_and(|path| self.model.file_entry(path).is_none())
                        {
                            (None, true)
                        } else {
                            (Some(("Could not open file".to_owned(), error)), true)
                        }
                    }
                }
            }
            ExplorerOutcome::WindowHighlighted {
                id,
                document_id,
                result,
            } => {
                let pending_cleared = self.pending_window == Some((id, document_id));
                if pending_cleared {
                    self.pending_window = None;
                }
                let content_changed = self
                    .model
                    .viewer
                    .as_mut()
                    .filter(|viewer| viewer.document_id == document_id)
                    .is_some_and(|viewer| viewer.install_syntax(result));
                if content_changed {
                    self.content_revision = self.content_revision.saturating_add(1);
                }
                (None, pending_cleared || content_changed)
            }
            ExplorerOutcome::Paths { .. }
            | ExplorerOutcome::QuickOpenPaths { .. }
            | ExplorerOutcome::FileLoaded { .. } => (None, false),
        }
    }

    #[must_use]
    pub fn is_preparing(&self) -> bool {
        self.paths_pending
            || self.quick_open_paths_pending
            || self.pending_path.is_some()
            || self.pending_window.is_some()
            || !self.queued.is_empty()
    }

    fn pending_request_id(&self) -> Option<u64> {
        self.pending_path
            .as_ref()
            .map(|_| self.latest_load)
            .or_else(|| self.pending_window.map(|(id, _)| id))
    }

    fn viewer_syntax_ready(&self) -> bool {
        self.viewer_syntax_ready_at(self.model.viewer_scroll)
    }

    fn viewer_syntax_ready_at(&self, target: usize) -> bool {
        let Some(viewer) = self.model.viewer.as_ref() else {
            return true;
        };
        if !viewer.syntax_eligible {
            return true;
        }
        let start = u32::try_from(target.saturating_add(1)).unwrap_or(u32::MAX);
        let end = u32::try_from(
            target
                .saturating_add(self.viewport_rows)
                .min(viewer.lines.len()),
        )
        .unwrap_or(u32::MAX);
        viewer
            .coverage
            .covers(Some(diffo_highlight::LineRange::new(start, end)))
    }
}

#[cfg(test)]
mod tests;
