//! Checkout commit history, selection, patch preparation, and rendering.

mod prepare;
mod view;

use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread,
};

use crate::diff::ReviewSelection;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_core::{CheckoutHistory, Commit, HeadState, RepositoryQueryId, RepositorySnapshot};
use diffo_highlight::SyntaxHighlighter;
use diffo_ui::{
    PaneSplit, design,
    file_picker::{FilePicker, Outcome as PickerOutcome},
    text_view::{
        LINE_SCROLL_ROWS, PreparedVerticalScroll, ScrollCommand, ScrollbarAxis, TextRenderMode,
        TextSurface, TextSurfacePreparation, Viewport, scrollbar_areas, scrollbar_axis_at,
        scrollbar_command, syntax_prefetch_viewports, viewport_metrics, wheel_scroll_command,
    },
};
use ratatui::{Frame, layout::Rect, text::Line};

use prepare::{PrepareOutcome, PrepareRequest, PreparedPatch};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HistoryRequest {
    Commits {
        query_id: RepositoryQueryId,
    },
    Patch {
        query_id: RepositoryQueryId,
        commit_id: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryEvent {
    Consumed,
}

pub struct HistoryActivity {
    head_commit: Option<String>,
    commits: Vec<Commit>,
    pending_commits: Option<Vec<Commit>>,
    picker: FilePicker<String>,
    patch: Option<PreparedPatch>,
    selection: Option<ReviewSelection>,
    pending_selection: Option<ReviewSelection>,
    queued: VecDeque<HistoryRequest>,
    next_id: u64,
    latest_history: u64,
    latest_patch: u64,
    latest_prepare: u64,
    history_pending: bool,
    patch_pending: bool,
    prepare_pending: bool,
    prepare_tx: Sender<PrepareRequest>,
    prepare_rx: Receiver<PrepareOutcome>,
    latest_prepare_id: Arc<AtomicU64>,
    viewport_rows: usize,
    vertical_scroll: PreparedVerticalScroll,
    scroll: usize,
    horizontal: usize,
    scrollbar_drag: Option<ScrollbarAxis>,
    content_revision: u64,
}

impl HistoryActivity {
    #[must_use]
    pub fn new(snapshot: &RepositorySnapshot) -> Self {
        let (prepare_tx, requests) = channel::<PrepareRequest>();
        let (outcomes, prepare_rx) = channel();
        let latest_prepare_id = Arc::new(AtomicU64::new(0));
        let worker_latest = Arc::clone(&latest_prepare_id);
        thread::spawn(move || {
            let highlighter = SyntaxHighlighter::new();
            while let Ok(request) = requests.recv() {
                if request.id != worker_latest.load(Ordering::Acquire) {
                    continue;
                }
                let outcome = prepare::prepare(request, &highlighter);
                if outcome.id == worker_latest.load(Ordering::Acquire)
                    && outcomes.send(outcome).is_err()
                {
                    break;
                }
            }
        });
        let mut activity = Self {
            head_commit: snapshot_head(snapshot),
            commits: Vec::new(),
            pending_commits: None,
            picker: FilePicker::default(),
            patch: None,
            selection: None,
            pending_selection: None,
            queued: VecDeque::new(),
            next_id: 0,
            latest_history: 0,
            latest_patch: 0,
            latest_prepare: 0,
            history_pending: false,
            patch_pending: false,
            prepare_pending: false,
            prepare_tx,
            prepare_rx,
            latest_prepare_id,
            viewport_rows: 1,
            vertical_scroll: PreparedVerticalScroll::default(),
            scroll: 0,
            horizontal: 0,
            scrollbar_drag: None,
            content_revision: 0,
        };
        activity.request_history();
        activity
    }

    fn next_id(&mut self) -> u64 {
        self.next_id = self.next_id.saturating_add(1);
        self.next_id
    }

    fn request_history(&mut self) {
        let id = self.next_id();
        self.latest_history = id;
        self.history_pending = true;
        self.queued
            .retain(|request| !matches!(request, HistoryRequest::Commits { .. }));
        self.queued.push_back(HistoryRequest::Commits {
            query_id: RepositoryQueryId(id),
        });
    }

    fn request_patch(&mut self, commit_id: String) {
        let id = self.next_id();
        self.latest_patch = id;
        self.latest_prepare = id;
        self.latest_prepare_id.store(id, Ordering::Release);
        self.pending_selection = Some(ReviewSelection::CompleteChange(commit_id.clone()));
        self.patch_pending = true;
        self.prepare_pending = false;
        self.queued
            .retain(|request| !matches!(request, HistoryRequest::Patch { .. }));
        self.queued.push_back(HistoryRequest::Patch {
            query_id: RepositoryQueryId(id),
            commit_id,
        });
    }

    fn submit_prepare(
        &mut self,
        commit_id: String,
        summary: String,
        patch: Arc<str>,
        target_scroll: usize,
        window_viewports: usize,
    ) {
        let id = self.next_id();
        self.latest_prepare = id;
        self.latest_prepare_id.store(id, Ordering::Release);
        self.prepare_pending = true;
        let _ = self.prepare_tx.send(PrepareRequest {
            id,
            commit_id,
            summary,
            patch,
            target_scroll,
            viewport_rows: self.viewport_rows,
            window_viewports,
        });
    }

    pub fn repository_changed(&mut self, snapshot: &RepositorySnapshot) {
        let head = snapshot_head(snapshot);
        if head == self.head_commit {
            return;
        }
        self.head_commit = head;
        self.pending_commits = None;
        self.pending_selection = None;
        self.patch_pending = false;
        self.prepare_pending = false;
        let prepare_id = self.next_id();
        self.latest_prepare = prepare_id;
        self.latest_prepare_id.store(prepare_id, Ordering::Release);
        self.vertical_scroll.clear();
        self.request_history();
    }

    pub fn accept_history(
        &mut self,
        query_id: RepositoryQueryId,
        history: CheckoutHistory,
    ) -> bool {
        if query_id.0 != self.latest_history || history.head_commit != self.head_commit {
            return false;
        }
        self.history_pending = false;
        if history.commits.is_empty() {
            let changed = !self.commits.is_empty() || self.patch.is_some();
            self.commits.clear();
            self.pending_commits = None;
            self.selection = None;
            self.pending_selection = None;
            self.patch = None;
            self.scroll = 0;
            self.horizontal = 0;
            if changed {
                self.content_revision = self.content_revision.saturating_add(1);
            }
            return changed;
        }
        let target = self
            .selection
            .as_ref()
            .and_then(ReviewSelection::complete_change_id)
            .filter(|selected| history.commits.iter().any(|commit| commit.id == *selected))
            .map_or_else(|| history.commits[0].id.clone(), str::to_owned);
        if self
            .patch
            .as_ref()
            .is_some_and(|patch| patch.commit_id == target)
        {
            let changed = self.commits != history.commits;
            self.commits = history.commits;
            if changed {
                self.content_revision = self.content_revision.saturating_add(1);
            }
            return changed;
        }
        self.pending_commits = Some(history.commits);
        self.request_patch(target);
        true
    }

    pub fn history_failed(&mut self, query_id: RepositoryQueryId) -> bool {
        if query_id.0 != self.latest_history {
            return false;
        }
        self.history_pending = false;
        true
    }

    pub fn accept_patch(
        &mut self,
        query_id: RepositoryQueryId,
        commit_id: String,
        patch: String,
    ) -> bool {
        if query_id.0 != self.latest_patch
            || self
                .pending_selection
                .as_ref()
                .and_then(ReviewSelection::complete_change_id)
                != Some(commit_id.as_str())
        {
            return false;
        }
        self.patch_pending = false;
        let Some(summary) = self.commit_summary(&commit_id).map(str::to_owned) else {
            return false;
        };
        self.submit_prepare(commit_id, summary, Arc::from(patch), 0, 3);
        true
    }

    pub fn patch_failed(&mut self, query_id: RepositoryQueryId, commit_id: &str) -> bool {
        if query_id.0 != self.latest_patch
            || self
                .pending_selection
                .as_ref()
                .and_then(ReviewSelection::complete_change_id)
                != Some(commit_id)
        {
            return false;
        }
        self.patch_pending = false;
        self.pending_selection = None;
        self.pending_commits = None;
        true
    }

    fn commit_summary(&self, commit_id: &str) -> Option<&str> {
        self.pending_commits
            .as_deref()
            .unwrap_or(&self.commits)
            .iter()
            .find(|commit| commit.id == commit_id)
            .map(|commit| commit.summary.as_str())
    }

    fn drain_prepared(&mut self) -> bool {
        let mut changed = false;
        while let Ok(outcome) = self.prepare_rx.try_recv() {
            changed |= self.install_prepared(outcome);
        }
        changed
    }

    fn install_prepared(&mut self, outcome: PrepareOutcome) -> bool {
        if outcome.id != self.latest_prepare {
            return false;
        }
        let selection_ready = self
            .pending_selection
            .as_ref()
            .and_then(ReviewSelection::complete_change_id)
            == Some(outcome.prepared.commit_id.as_str());
        let scroll_ready = self.pending_selection.is_none()
            && self
                .patch
                .as_ref()
                .is_some_and(|patch| patch.commit_id == outcome.prepared.commit_id);
        if !selection_ready && !scroll_ready {
            return false;
        }
        self.prepare_pending = false;
        if selection_ready {
            if let Some(commits) = self.pending_commits.take() {
                self.commits = commits;
            }
            self.selection = self.pending_selection.take();
            self.horizontal = 0;
        }
        self.scroll = outcome.prepared.target_scroll;
        self.patch = Some(outcome.prepared);
        self.vertical_scroll.clear();
        self.content_revision = self.content_revision.saturating_add(1);
        true
    }

    pub fn prepare_frame(&mut self, area: Rect, split: PaneSplit) -> TextSurfacePreparation {
        let _ = self.drain_prepared();
        let areas = view::areas(area, split);
        let inner = areas.patch.inner(design::PANEL_INSET);
        self.viewport_rows = usize::from(inner.height).max(1);
        self.picker.prepare(
            areas.commits,
            view::commit_document(
                &self.commits,
                split.border_style(),
                self.history_pending || (self.commits.is_empty() && self.pending_commits.is_some()),
            ),
            self.selection
                .as_ref()
                .and_then(|selection| match selection {
                    ReviewSelection::File(_) => None,
                    ReviewSelection::CompleteChange(id) => Some(id),
                }),
        );
        let metrics = view::patch_metrics(areas.patch, self.patch.as_ref(), self.scroll);
        self.finish_preparation(metrics)
    }

    fn finish_preparation(
        &mut self,
        metrics: diffo_ui::text_view::ViewportMetrics,
    ) -> TextSurfacePreparation {
        self.scroll = self.scroll.min(metrics.maximum_vertical);
        self.horizontal = self.horizontal.min(metrics.maximum_horizontal);
        if let Some(target) = self.vertical_scroll.requested() {
            let ready = self
                .patch
                .as_ref()
                .is_some_and(|patch| patch.syntax_ready(target, self.viewport_rows));
            if ready {
                self.scroll = self.vertical_scroll.take_ready(true).unwrap_or(self.scroll);
            } else if !self.prepare_pending
                && self.pending_selection.is_none()
                && let Some(patch) = self.patch.as_ref()
            {
                let commit_id = patch.commit_id.clone();
                let summary = patch.summary.clone();
                let raw = Arc::clone(&patch.patch);
                let viewports = syntax_prefetch_viewports(self.scroll, target, self.viewport_rows);
                self.submit_prepare(commit_id, summary, raw, target, viewports);
            }
        }
        let coverage = self.patch.as_ref().map(|patch| {
            (
                u32::try_from(patch.syntax_coverage.start).unwrap_or(u32::MAX),
                u32::try_from(patch.syntax_coverage.end).unwrap_or(u32::MAX),
            )
        });
        TextSurfacePreparation {
            surface: TextSurface::History,
            document_revision: self.content_revision,
            viewport: (self.scroll, self.viewport_rows),
            requested_range: (
                self.vertical_scroll.requested().unwrap_or(self.scroll),
                self.vertical_scroll
                    .requested()
                    .unwrap_or(self.scroll)
                    .saturating_add(self.viewport_rows),
            ),
            mode: if self.patch_pending || (self.patch.is_none() && self.prepare_pending) {
                TextRenderMode::TextSkeleton
            } else if self.prepare_pending {
                TextRenderMode::SyntaxSkeleton
            } else {
                TextRenderMode::Full
            },
            coverage_before: coverage,
            coverage_after: coverage,
            request_id: (self.patch_pending || self.prepare_pending).then_some(
                if self.prepare_pending {
                    self.latest_prepare
                } else {
                    self.latest_patch
                },
            ),
            cache_hit: !self.patch_pending && !self.prepare_pending,
            coalesced_request: false,
            stale_discarded: false,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, split: PaneSplit) {
        view::render(
            frame,
            area,
            split,
            &self.picker,
            self.patch.as_ref(),
            self.scroll,
            self.horizontal,
        );
    }

    pub fn prepare_full_screen(&mut self, area: Rect) -> TextSurfacePreparation {
        let _ = self.drain_prepared();
        self.viewport_rows = usize::from(area.height).max(1);
        let metrics = self.patch.as_ref().map_or_else(
            || viewport_metrics(area, &[], self.scroll, true),
            |patch| viewport_metrics(area, &patch.widths, self.scroll, true),
        );
        self.finish_preparation(metrics)
    }

    pub fn render_full_screen(&self, frame: &mut Frame, area: Rect) {
        if let Some(patch) = self.patch.as_ref() {
            view::render_full_screen(frame, area, patch, self.scroll, self.horizontal);
        }
    }

    #[must_use]
    pub fn full_screen_title(&self) -> Option<Line<'static>> {
        self.patch.as_ref().map(PreparedPatch::title)
    }

    pub fn handle_event(
        &mut self,
        event: &Event,
        area: Rect,
        split: PaneSplit,
    ) -> Option<HistoryEvent> {
        let selected_before = self.picker.selected().cloned();
        if let Some(outcome) = self.picker.handle_event(event, area) {
            let consumed = matches!(&outcome, PickerOutcome::Consumed);
            match outcome {
                PickerOutcome::Selected(commit_id) | PickerOutcome::Activated(commit_id)
                    if self.patch.as_ref().map(|patch| &patch.commit_id) != Some(&commit_id) =>
                {
                    self.request_patch(commit_id);
                }
                PickerOutcome::Consumed
                | PickerOutcome::Selected(_)
                | PickerOutcome::Activated(_)
                | PickerOutcome::RowAction(_)
                | PickerOutcome::PanelAction
                | PickerOutcome::CopyPath { .. }
                | PickerOutcome::DestructiveAction(_) => {}
            }
            return (self.picker.selected() != selected_before.as_ref() || consumed)
                .then_some(HistoryEvent::Consumed);
        }
        if self.handle_patch_mouse(event, area, split) {
            return Some(HistoryEvent::Consumed);
        }
        let Event::Key(key) = event else {
            return None;
        };
        if key.kind != KeyEventKind::Press || key.modifiers != KeyModifiers::NONE {
            return None;
        }
        let command = match key.code {
            KeyCode::Up => ScrollCommand::Lines(-LINE_SCROLL_ROWS),
            KeyCode::Down => ScrollCommand::Lines(LINE_SCROLL_ROWS),
            KeyCode::PageUp => ScrollCommand::Page(-1),
            KeyCode::PageDown => ScrollCommand::Page(1),
            KeyCode::Left => ScrollCommand::Columns(-LINE_SCROLL_ROWS),
            KeyCode::Right => ScrollCommand::Columns(LINE_SCROLL_ROWS),
            _ => return None,
        };
        self.apply_patch_command(command, area, split)
            .then_some(HistoryEvent::Consumed)
    }

    pub fn handle_full_screen_event(&mut self, event: &Event, area: Rect) -> Option<HistoryEvent> {
        let command = match event {
            Event::Key(key)
                if key.kind == KeyEventKind::Press && key.modifiers == KeyModifiers::NONE =>
            {
                match key.code {
                    KeyCode::Up => ScrollCommand::Lines(-LINE_SCROLL_ROWS),
                    KeyCode::Down => ScrollCommand::Lines(LINE_SCROLL_ROWS),
                    KeyCode::PageUp => ScrollCommand::Page(-1),
                    KeyCode::PageDown => ScrollCommand::Page(1),
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
        self.apply_full_screen_command(command, area)
            .then_some(HistoryEvent::Consumed)
    }

    fn handle_patch_mouse(&mut self, event: &Event, area: Rect, split: PaneSplit) -> bool {
        let patch_area = view::areas(area, split).patch;
        let inner = patch_area.inner(design::PANEL_INSET);
        let metrics = view::patch_metrics(patch_area, self.patch.as_ref(), self.scroll);
        let Event::Mouse(mouse) = event else {
            return false;
        };
        if mouse.kind == MouseEventKind::Up(MouseButton::Left) && self.scrollbar_drag.is_some() {
            self.scrollbar_drag = None;
            return true;
        }
        let areas = scrollbar_areas(inner, metrics);
        let axis = if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            scrollbar_axis_at(areas, metrics, mouse.column, mouse.row)
        } else if mouse.kind == MouseEventKind::Drag(MouseButton::Left) {
            self.scrollbar_drag
        } else {
            None
        };
        if let Some(axis) = axis {
            self.scrollbar_drag = Some(axis);
            return self.apply_patch_command(
                scrollbar_command(axis, areas, metrics, mouse.column, mouse.row),
                area,
                split,
            );
        }
        inner
            .contains((mouse.column, mouse.row).into())
            .then(|| wheel_scroll_command(mouse.kind))
            .flatten()
            .is_some_and(|command| self.apply_patch_command(command, area, split))
    }

    fn apply_patch_command(
        &mut self,
        command: ScrollCommand,
        area: Rect,
        split: PaneSplit,
    ) -> bool {
        let metrics = view::patch_metrics(
            view::areas(area, split).patch,
            self.patch.as_ref(),
            self.scroll,
        );
        self.apply_command(command, metrics)
    }

    fn apply_full_screen_command(&mut self, command: ScrollCommand, area: Rect) -> bool {
        let metrics = self.patch.as_ref().map_or_else(
            || viewport_metrics(area, &[], self.scroll, true),
            |patch| viewport_metrics(area, &patch.widths, self.scroll, true),
        );
        self.apply_command(command, metrics)
    }

    fn apply_command(
        &mut self,
        command: ScrollCommand,
        metrics: diffo_ui::text_view::ViewportMetrics,
    ) -> bool {
        if matches!(
            command,
            ScrollCommand::Columns(_) | ScrollCommand::Horizontal(_)
        ) {
            let before = self.horizontal;
            let mut viewport = Viewport {
                vertical: self.scroll,
                horizontal: self.horizontal,
            };
            viewport.apply(command, metrics);
            self.horizontal = viewport.horizontal;
            return before != self.horizontal;
        }
        let before = self.vertical_scroll.requested().unwrap_or(self.scroll);
        let after = self
            .vertical_scroll
            .request(command, self.scroll, metrics)
            .unwrap_or(before);
        before != after
    }

    pub fn take_request(&mut self) -> Option<HistoryRequest> {
        self.queued.pop_front()
    }

    #[must_use]
    pub fn document_commits(&self) -> (Option<String>, Option<String>, Option<String>) {
        (
            self.pending_selection
                .as_ref()
                .and_then(ReviewSelection::complete_change_id)
                .map(str::to_owned)
                .or_else(|| {
                    self.selection
                        .as_ref()
                        .and_then(ReviewSelection::complete_change_id)
                        .map(str::to_owned)
                }),
            self.picker.selected().cloned(),
            self.selection
                .as_ref()
                .and_then(ReviewSelection::complete_change_id)
                .map(str::to_owned),
        )
    }

    #[must_use]
    pub fn is_preparing(&self) -> bool {
        self.history_pending
            || self.patch_pending
            || self.prepare_pending
            || !self.queued.is_empty()
    }

    #[must_use]
    pub fn help_rows(&self) -> Vec<(String, &'static str)> {
        vec![
            ("j".to_owned(), "Previous commit"),
            ("k / l".to_owned(), "Next commit"),
            ("f".to_owned(), "Toggle full-screen commit diff"),
            ("q / Esc / Ctrl+c".to_owned(), "Quit"),
            ("↑ / ↓".to_owned(), "Scroll commit diff by four lines"),
            (
                "Page Up / Page Down".to_owned(),
                "Scroll commit diff by one page",
            ),
            ("← / →".to_owned(), "Scroll commit diff horizontally"),
        ]
    }
}

fn snapshot_head(snapshot: &RepositorySnapshot) -> Option<String> {
    match &snapshot.head {
        HeadState::Named { commit, .. } | HeadState::Detached { commit } => Some(commit.clone()),
        HeadState::Unborn { .. } => None,
    }
}

#[cfg(test)]
mod tests;
