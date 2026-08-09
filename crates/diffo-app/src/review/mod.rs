//! AI-guided review state, input, requests, and rendering.

mod request;
mod view;

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_core::{CancellationHandle, RepositorySnapshot};
use diffo_ui::{PaneSplit, tool_areas};
use ratatui::{Frame, layout::Rect};

use crate::diff::{FramePreparation, Message, Model, Renderer, RendererEvent, update};

pub use request::{AttentionCategory, ReviewRequest, ReviewResult, ReviewStop};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodexAvailability {
    Available,
    Unavailable(String),
}

impl CodexAvailability {
    #[must_use]
    pub const fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }
}

#[derive(Clone, Debug)]
pub struct ReviewCodexTask {
    pub id: u64,
    pub request: ReviewRequest,
    pub cancellation: CancellationHandle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewCodexTaskResult {
    pub id: u64,
    pub outcome: ReviewCodexOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReviewCodexOutcome {
    Generated(ReviewResult),
    Failed(String),
    Cancelled,
}

pub(crate) enum ReviewEvent {
    Redraw,
    ToggleStage(crate::diff::FileKey),
}

struct CachedReview {
    request: ReviewRequest,
    result: ReviewResult,
}

struct ActiveRequest {
    id: u64,
    cancellation: CancellationHandle,
}

pub(crate) struct ReviewActivity {
    availability: CodexAvailability,
    model: Model,
    renderer: Renderer,
    cached: Option<CachedReview>,
    active_request: Option<ActiveRequest>,
    pending_task: Option<ReviewCodexTask>,
    next_request_id: u64,
    failure: Option<String>,
    selected_stop: usize,
    active_hunk_id: Option<String>,
    pending_hunk_id: Option<String>,
    stop_areas: Vec<(Rect, usize)>,
    generate_area: Rect,
}

impl ReviewActivity {
    #[must_use]
    pub(crate) fn new(snapshot: RepositorySnapshot, availability: CodexAvailability) -> Self {
        Self {
            availability,
            model: Model::new(snapshot),
            renderer: Renderer::new(),
            cached: None,
            active_request: None,
            pending_task: None,
            next_request_id: 1,
            failure: None,
            selected_stop: 0,
            active_hunk_id: None,
            pending_hunk_id: None,
            stop_areas: Vec::new(),
            generate_area: Rect::default(),
        }
    }

    #[must_use]
    pub(crate) fn available(&self) -> bool {
        self.availability.is_available()
    }

    #[must_use]
    pub(crate) fn unavailable_reason(&self) -> Option<&str> {
        match &self.availability {
            CodexAvailability::Available => None,
            CodexAvailability::Unavailable(reason) => Some(reason),
        }
    }

    pub(crate) fn repository_changed(&mut self, snapshot: RepositorySnapshot) {
        if self.model.snapshot == snapshot {
            return;
        }
        let rebound = self
            .cached
            .as_ref()
            .and_then(|cached| cached.request.rebind_staging(&snapshot));
        let _ = update(&mut self.model, Message::SnapshotLoaded(snapshot));
        if let (Some(cached), Some(request)) = (&mut self.cached, rebound) {
            cached.request = request;
            if let Some(id) = self.active_hunk_id.clone() {
                self.open_hunk(id.clone());
                self.pending_hunk_id = Some(id);
            }
            return;
        }
        if let Some(active) = self.active_request.take() {
            active.cancellation.cancel();
        }
        self.pending_task = None;
        self.pending_hunk_id = None;
        self.active_hunk_id = None;
        self.failure = None;
    }

    #[must_use]
    fn ready(&self) -> Option<&CachedReview> {
        self.cached
            .as_ref()
            .filter(|cached| cached.request.still_matches(&self.model.snapshot))
    }

    #[must_use]
    fn stale(&self) -> bool {
        self.cached
            .as_ref()
            .is_some_and(|cached| !cached.request.still_matches(&self.model.snapshot))
    }

    pub(crate) fn handle_event(
        &mut self,
        event: &Event,
        area: Rect,
        split: PaneSplit,
    ) -> Option<ReviewEvent> {
        if !self.available() {
            return None;
        }
        if self.active_request.is_some() {
            if plain_key(event, KeyCode::Enter) {
                self.cancel_active();
                return Some(ReviewEvent::Redraw);
            }
            return None;
        }
        if self.ready().is_none() {
            if plain_key(event, KeyCode::Enter) || clicked(event, self.generate_area) {
                self.start_generation();
                return Some(ReviewEvent::Redraw);
            }
            return None;
        }

        if plain_key(event, KeyCode::Char('j')) {
            self.select_next();
            return Some(ReviewEvent::Redraw);
        }
        if plain_key(event, KeyCode::Char('k')) {
            self.select_previous();
            return Some(ReviewEvent::Redraw);
        }
        if plain_key(event, KeyCode::Enter) {
            self.open_selected_stop();
            return Some(ReviewEvent::Redraw);
        }
        if plain_key(event, KeyCode::Char(' '))
            && let Some(file) = self.active_file()
        {
            return Some(ReviewEvent::ToggleStage(file));
        }
        if let Event::Mouse(mouse) = event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && let Some((_, index)) = self
                .stop_areas
                .iter()
                .find(|(target, _)| target.contains((mouse.column, mouse.row).into()))
        {
            self.selected_stop = *index;
            self.open_selected_stop();
            return Some(ReviewEvent::Redraw);
        }

        let trailing = split.areas(tool_areas(area).content).trailing;
        let renderer_event = self
            .renderer
            .map_review_buffer_event(event, &self.model, trailing)?;
        match renderer_event {
            RendererEvent::Message(Message::Quit) | RendererEvent::CopyPath { .. } => None,
            RendererEvent::Message(message) => {
                let _ = update(&mut self.model, message);
                Some(ReviewEvent::Redraw)
            }
            RendererEvent::Consumed => Some(ReviewEvent::Redraw),
        }
    }

    fn start_generation(&mut self) {
        let Some(request) = ReviewRequest::from_snapshot(&self.model.snapshot) else {
            self.failure = Some("There are no staged or unstaged changes to review.".to_owned());
            return;
        };
        let id = self.next_id();
        let cancellation = CancellationHandle::default();
        self.active_request = Some(ActiveRequest {
            id,
            cancellation: cancellation.clone(),
        });
        self.pending_task = Some(ReviewCodexTask {
            id,
            request,
            cancellation,
        });
        self.failure = None;
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        id
    }

    fn cancel_active(&mut self) {
        if let Some(active) = &self.active_request {
            active.cancellation.cancel();
        }
    }

    fn select_next(&mut self) {
        let count = self.ready().map_or(0, |cached| cached.result.stops.len());
        self.selected_stop = self
            .selected_stop
            .saturating_add(1)
            .min(count.saturating_sub(1));
    }

    fn select_previous(&mut self) {
        self.selected_stop = self.selected_stop.saturating_sub(1);
    }

    fn open_selected_stop(&mut self) {
        let id = self.ready().and_then(|cached| {
            cached
                .result
                .stops
                .get(self.selected_stop)
                .map(|stop| stop.primary_hunk_id.clone())
        });
        if let Some(id) = id {
            self.open_hunk(id);
        }
    }

    fn open_hunk(&mut self, id: String) {
        let hunk = self
            .ready()
            .and_then(|cached| cached.request.hunk(&id))
            .cloned();
        let Some(hunk) = hunk else {
            return;
        };
        self.model.select_file(&hunk.file);
        self.pending_hunk_id = Some(id.clone());
        self.active_hunk_id = Some(id);
    }

    fn active_file(&self) -> Option<crate::diff::FileKey> {
        self.active_hunk_id
            .as_ref()
            .and_then(|id| self.ready()?.request.hunk(id))
            .map(|hunk| hunk.file.clone())
    }

    pub(crate) fn take_task(&mut self) -> Option<ReviewCodexTask> {
        self.pending_task.take()
    }

    pub(crate) fn accept(&mut self, result: ReviewCodexTaskResult) -> bool {
        let Some(active) = self
            .active_request
            .take()
            .filter(|active| active.id == result.id)
        else {
            return false;
        };
        if active.cancellation.is_cancelled() {
            return true;
        }
        match result.outcome {
            ReviewCodexOutcome::Generated(review) => {
                let Some(request) = ReviewRequest::from_snapshot(&self.model.snapshot) else {
                    return true;
                };
                self.cached = Some(CachedReview {
                    request,
                    result: review,
                });
                self.failure = None;
                self.selected_stop = 0;
                self.open_selected_stop();
            }
            ReviewCodexOutcome::Failed(error) => {
                self.failure = Some(error);
            }
            ReviewCodexOutcome::Cancelled => {}
        }
        true
    }

    pub(crate) fn prepare_frame(&mut self, area: Rect, split: PaneSplit) -> FramePreparation {
        let panes = split.areas(tool_areas(area).content);
        let target = self.pending_hunk_id.as_ref().and_then(|id| {
            self.ready()
                .and_then(|cached| cached.request.hunk(id))
                .map(|hunk| hunk.target)
        });
        if self.active_hunk_id.is_none() || self.ready().is_none() {
            return FramePreparation::default();
        }
        let preparation = self
            .renderer
            .prepare_review_buffer(&self.model, panes.trailing, target);
        if let Some(viewport) = preparation.viewport_transition {
            self.model
                .set_diff_viewport(viewport.vertical, viewport.horizontal);
        }
        self.model.clamp_diff_scroll(
            preparation.maximum_vertical_scroll,
            preparation.maximum_horizontal_scroll,
        );
        if !preparation.preparing
            && preparation.syntax_ready
            && target.is_some_and(|target| target == self.model.diff_scroll)
        {
            self.pending_hunk_id = None;
        }
        preparation
    }

    pub(crate) fn render(&mut self, frame: &mut Frame, area: Rect, split: PaneSplit) {
        let panes = split.areas(tool_areas(area).content);
        let hits = view::render_review(frame, panes.leading, self);
        self.stop_areas = hits.stop_areas;
        self.generate_area = hits.generate_area;
        if self.active_hunk_id.is_some() && self.ready().is_some() {
            self.renderer
                .render_review_buffer(frame, panes.trailing, &self.model);
        } else {
            view::render_empty_diff(frame, panes.trailing);
        }
    }

    #[must_use]
    pub(crate) fn is_preparing(&self) -> bool {
        self.active_request.is_some() || self.renderer.is_preparing()
    }

    pub(crate) fn help_rows() -> Vec<(String, &'static str)> {
        vec![
            ("j / k".to_owned(), "Select review stop"),
            ("Enter".to_owned(), "Open or generate"),
            ("Space".to_owned(), "Stage / unstage reviewed file"),
        ]
    }
}

fn plain_key(event: &Event, code: KeyCode) -> bool {
    matches!(
        event,
        Event::Key(key)
            if key.kind == KeyEventKind::Press
                && key.modifiers == KeyModifiers::NONE
                && key.code == code
    )
}

fn clicked(event: &Event, area: Rect) -> bool {
    matches!(
        event,
        Event::Mouse(mouse)
            if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                && area.contains((mouse.column, mouse.row).into())
    )
}

#[cfg(test)]
mod tests;
