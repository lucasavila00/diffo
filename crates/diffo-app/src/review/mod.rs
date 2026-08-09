//! AI-guided review state, input, requests, and rendering.

mod request;
mod view;

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_core::{ApplicationCommandId, CancellationHandle, RepositorySnapshot};
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewCodexTaskResult {
    pub id: ApplicationCommandId,
    pub outcome: ReviewCodexOutcome,
    pub complete: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewProgress {
    pub batch: usize,
    pub batches: usize,
    pub change_start: usize,
    pub change_end: usize,
    pub changes: usize,
    pub files: Vec<std::path::PathBuf>,
}

impl ReviewProgress {
    pub(crate) fn command_phase(&self) -> String {
        if self.change_start == self.change_end {
            format!(
                "Change {}/{} · part {}/{}",
                self.change_start, self.changes, self.batch, self.batches
            )
        } else {
            format!(
                "Changes {}-{}/{} · part {}/{}",
                self.change_start, self.change_end, self.changes, self.batch, self.batches
            )
        }
    }

    pub(crate) fn description(&self) -> String {
        if self.change_start == self.change_end {
            format!(
                "Reviewing change {} of {} · part {} of {}",
                self.change_start, self.changes, self.batch, self.batches
            )
        } else {
            format!(
                "Reviewing changes {}-{} of {} · part {} of {}",
                self.change_start, self.change_end, self.changes, self.batch, self.batches
            )
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReviewCodexOutcome {
    Generated(ReviewResult),
    Failed(String),
    Cancelled,
}

pub(crate) enum ReviewEvent {
    Redraw,
    Generate(ReviewRequest),
    Cancel(ApplicationCommandId),
    ToggleStage(crate::diff::FileKey),
    AiCommit,
}

struct CachedReview {
    request: ReviewRequest,
    result: ReviewResult,
}

struct ActiveRequest {
    id: ApplicationCommandId,
    request: ReviewRequest,
    cancellation: Option<CancellationHandle>,
    cancelling: bool,
    progress: Option<ReviewProgress>,
}

pub(crate) struct ReviewActivity {
    availability: CodexAvailability,
    model: Model,
    renderer: Renderer,
    cached: Option<CachedReview>,
    active_request: Option<ActiveRequest>,
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

    pub(crate) fn repository_changed(
        &mut self,
        snapshot: RepositorySnapshot,
    ) -> Option<ApplicationCommandId> {
        if self.model.snapshot == snapshot {
            return None;
        }
        if let Some(active) = self.active_request.as_mut() {
            if let Some(cancellation) = &active.cancellation {
                cancellation.cancel();
            }
            active.cancelling = true;
            let id = active.id;
            let _ = update(&mut self.model, Message::SnapshotLoaded(snapshot));
            self.clear_partial_review();
            self.failure = None;
            return Some(id);
        }
        let active_before = self.active_file();
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
            if active_before.is_some_and(|file| {
                file.area == crate::diff::ChangeArea::Unstaged
                    && self
                        .active_file()
                        .is_some_and(|current| current.area == crate::diff::ChangeArea::Staged)
            }) {
                self.open_next_stop();
            }
            return None;
        }
        self.pending_hunk_id = None;
        self.active_hunk_id = None;
        self.failure = None;
        None
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

    #[must_use]
    fn has_changes(&self) -> bool {
        self.model
            .snapshot
            .files
            .iter()
            .any(|file| file.staged.is_some() || file.unstaged.is_some())
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
        let generating = self.active_request.is_some();
        if generating && plain_key(event, KeyCode::Enter) {
            return self
                .active_request
                .as_ref()
                .map(|active| ReviewEvent::Cancel(active.id));
        }
        if !generating && plain_key(event, KeyCode::Char('i')) {
            return Some(ReviewEvent::AiCommit);
        }
        if self.ready().is_none() {
            if !generating
                && (plain_key(event, KeyCode::Enter) || clicked(event, self.generate_area))
            {
                return self.generation_request().map(ReviewEvent::Generate);
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
        if !generating && plain_key(event, KeyCode::Enter) {
            self.open_selected_stop();
            return Some(ReviewEvent::Redraw);
        }
        if !generating
            && plain_key(event, KeyCode::Char(' '))
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

    fn generation_request(&mut self) -> Option<ReviewRequest> {
        let Some(request) = ReviewRequest::from_snapshot(&self.model.snapshot) else {
            self.failure = Some("There are no staged or unstaged changes to review.".to_owned());
            return None;
        };
        self.failure = None;
        Some(request)
    }

    pub(crate) fn generation_queued(&mut self, id: ApplicationCommandId, request: ReviewRequest) {
        self.cached = None;
        self.active_request = Some(ActiveRequest {
            id,
            request,
            cancellation: None,
            cancelling: false,
            progress: None,
        });
        self.failure = None;
        self.selected_stop = 0;
        self.active_hunk_id = None;
        self.pending_hunk_id = None;
    }

    pub(crate) fn generation_started(
        &mut self,
        id: ApplicationCommandId,
        cancellation: CancellationHandle,
    ) {
        if let Some(active) = self
            .active_request
            .as_mut()
            .filter(|active| active.id == id)
        {
            active.cancellation = Some(cancellation);
        }
    }

    pub(crate) fn generation_cancelling(&mut self, id: ApplicationCommandId) {
        if let Some(active) = self
            .active_request
            .as_mut()
            .filter(|active| active.id == id)
        {
            active.cancelling = true;
        }
    }

    pub(crate) fn generation_progress(
        &mut self,
        id: ApplicationCommandId,
        progress: ReviewProgress,
    ) -> bool {
        let Some(active) = self
            .active_request
            .as_mut()
            .filter(|active| active.id == id && !active.cancelling)
        else {
            return false;
        };
        active.progress = Some(progress);
        true
    }

    pub(crate) fn generation_cancelled_before_start(&mut self, id: ApplicationCommandId) {
        if self
            .active_request
            .as_ref()
            .is_some_and(|active| active.id == id)
        {
            self.active_request = None;
        }
    }

    pub(crate) fn generation_rejected(&mut self, detail: impl Into<String>) {
        self.failure = Some(detail.into());
    }

    fn select_next(&mut self) {
        let count = self.ready().map_or(0, |cached| cached.result.stops.len());
        let selected = self
            .selected_stop
            .saturating_add(1)
            .min(count.saturating_sub(1));
        if selected != self.selected_stop {
            self.selected_stop = selected;
            self.open_selected_stop();
        }
    }

    fn select_previous(&mut self) {
        let selected = self.selected_stop.saturating_sub(1);
        if selected != self.selected_stop {
            self.selected_stop = selected;
            self.open_selected_stop();
        }
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

    fn open_next_stop(&mut self) {
        let count = self.ready().map_or(0, |cached| cached.result.stops.len());
        let next = self.selected_stop.saturating_add(1);
        if next < count {
            self.selected_stop = next;
            self.open_selected_stop();
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

    pub(crate) fn accept(&mut self, result: ReviewCodexTaskResult) -> bool {
        let Some(active) = self
            .active_request
            .as_ref()
            .filter(|active| active.id == result.id)
        else {
            return false;
        };
        let request = active.request.clone();
        let cancelled = active
            .cancellation
            .as_ref()
            .is_some_and(CancellationHandle::is_cancelled);
        if cancelled {
            if result.complete {
                self.clear_partial_review();
                self.active_request = None;
            }
            return true;
        }
        match result.outcome {
            ReviewCodexOutcome::Generated(review) => {
                let first = self.cached.is_none();
                self.merge_review(request, review);
                self.failure = None;
                if first {
                    self.selected_stop = 0;
                    self.open_selected_stop();
                }
            }
            ReviewCodexOutcome::Failed(error) => {
                self.clear_partial_review();
                self.failure = Some(error);
            }
            ReviewCodexOutcome::Cancelled => self.clear_partial_review(),
        }
        if result.complete {
            self.active_request = None;
        }
        true
    }

    fn clear_partial_review(&mut self) {
        self.cached = None;
        self.selected_stop = 0;
        self.active_hunk_id = None;
        self.pending_hunk_id = None;
    }

    fn merge_review(&mut self, request: ReviewRequest, review: ReviewResult) {
        let cached = self.cached.get_or_insert_with(|| CachedReview {
            request,
            result: ReviewResult {
                overview: Vec::new(),
                stops: Vec::new(),
            },
        });
        for overview in review.overview {
            if cached.result.overview.len() == 3 {
                break;
            }
            if !cached.result.overview.contains(&overview) {
                cached.result.overview.push(overview);
            }
        }
        for stop in review.stops {
            if cached.result.stops.len() == 8 {
                break;
            }
            if !cached
                .result
                .stops
                .iter()
                .any(|known| known.primary_hunk_id == stop.primary_hunk_id)
            {
                cached.result.stops.push(stop);
            }
        }
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
            view::render_empty_diff(frame, panes.trailing, self);
        }
    }

    #[must_use]
    pub(crate) fn is_preparing(&self) -> bool {
        self.active_request.is_some() || self.renderer.is_preparing()
    }

    pub(crate) fn help_rows() -> Vec<(String, &'static str)> {
        vec![
            ("j / k".to_owned(), "Previous / next review step"),
            ("Enter".to_owned(), "Start or recenter review"),
            ("Space".to_owned(), "Stage / unstage current file"),
            ("i".to_owned(), "AI commit staged changes"),
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
