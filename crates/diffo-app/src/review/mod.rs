//! AI-guided review state, input, requests, and rendering.

mod request;
mod view;

use std::collections::HashSet;

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_core::{CancellationHandle, RepositorySnapshot};
use diffo_ui::{PaneSplit, tool_areas};
use ratatui::{Frame, layout::Rect};

use crate::diff::{FramePreparation, Message, Model, Renderer, RendererEvent, update};

pub use request::{
    AskRequest, AskResult, AttentionCategory, ReviewRequest, ReviewResult, ReviewStop,
};

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
    pub request: ReviewCodexRequest,
    pub cancellation: CancellationHandle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReviewCodexRequest {
    Generate(ReviewRequest),
    Ask(AskRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewCodexTaskResult {
    pub id: u64,
    pub outcome: ReviewCodexOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReviewCodexOutcome {
    Generated(ReviewResult),
    Answered(AskResult),
    Failed(String),
    Cancelled,
}

struct CachedReview {
    request: ReviewRequest,
    result: ReviewResult,
}

struct ActiveRequest {
    id: u64,
    cancellation: CancellationHandle,
    kind: ActiveRequestKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActiveRequestKind {
    Generate,
    Ask,
}

#[derive(Default)]
enum AskState {
    #[default]
    Closed,
    Editing {
        question: String,
    },
    Running {
        question: String,
    },
    Answered {
        question: String,
        answer: AskResult,
        selected_link: usize,
    },
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
    visited: HashSet<String>,
    active_hunk_id: Option<String>,
    pending_hunk_id: Option<String>,
    ask: AskState,
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
            visited: HashSet::new(),
            active_hunk_id: None,
            pending_hunk_id: None,
            ask: AskState::Closed,
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
        let _ = update(&mut self.model, Message::SnapshotLoaded(snapshot));
        if let Some(active) = self.active_request.take() {
            active.cancellation.cancel();
        }
        self.pending_task = None;
        self.pending_hunk_id = None;
        self.active_hunk_id = None;
        self.ask = AskState::Closed;
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

    pub(crate) fn handle_event(&mut self, event: &Event, area: Rect, split: PaneSplit) -> bool {
        if !self.available() {
            return false;
        }
        if !matches!(self.ask, AskState::Closed) {
            return self.handle_ask_event(event);
        }
        if self.active_request.is_some() {
            if plain_key(event, KeyCode::Enter) {
                self.cancel_active();
                return true;
            }
            return false;
        }
        if self.ready().is_none() {
            if plain_key(event, KeyCode::Enter) || clicked(event, self.generate_area) {
                self.start_generation();
                return true;
            }
            return false;
        }

        if plain_key(event, KeyCode::Char('j')) {
            self.select_next();
            return true;
        }
        if plain_key(event, KeyCode::Char('k')) {
            self.select_previous();
            return true;
        }
        if plain_key(event, KeyCode::Enter) {
            self.open_selected_stop();
            return true;
        }
        if plain_key(event, KeyCode::Char('n')) {
            self.select_next();
            self.open_selected_stop();
            return true;
        }
        if plain_key(event, KeyCode::Char('p')) {
            self.select_previous();
            self.open_selected_stop();
            return true;
        }
        if plain_key(event, KeyCode::Char('/')) {
            self.ask = AskState::Editing {
                question: String::new(),
            };
            return true;
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
            return true;
        }

        let trailing = split.areas(tool_areas(area).content).trailing;
        let Some(renderer_event) =
            self.renderer
                .map_review_buffer_event(event, &self.model, trailing)
        else {
            return false;
        };
        match renderer_event {
            RendererEvent::Message(Message::Quit) | RendererEvent::CopyPath { .. } => false,
            RendererEvent::Message(message) => {
                let _ = update(&mut self.model, message);
                true
            }
            RendererEvent::Consumed => true,
        }
    }

    fn handle_ask_event(&mut self, event: &Event) -> bool {
        if plain_key(event, KeyCode::Esc) {
            if self
                .active_request
                .as_ref()
                .is_some_and(|active| active.kind == ActiveRequestKind::Ask)
            {
                self.cancel_active();
            }
            self.ask = AskState::Closed;
            return true;
        }
        match &mut self.ask {
            AskState::Editing { question } => match event {
                Event::Key(key)
                    if key.kind == KeyEventKind::Press
                        && key.modifiers == KeyModifiers::NONE
                        && key.code == KeyCode::Backspace =>
                {
                    question.pop();
                    true
                }
                Event::Key(key)
                    if key.kind == KeyEventKind::Press
                        && key.modifiers == KeyModifiers::NONE
                        && matches!(key.code, KeyCode::Char(_)) =>
                {
                    if let KeyCode::Char(character) = key.code
                        && question.chars().count() < 500
                    {
                        question.push(character);
                    }
                    true
                }
                Event::Key(key)
                    if key.kind == KeyEventKind::Press && key.code == KeyCode::Enter =>
                {
                    self.start_question();
                    true
                }
                _ => false,
            },
            AskState::Running { .. } | AskState::Closed => false,
            AskState::Answered {
                answer,
                selected_link,
                ..
            } => {
                if plain_key(event, KeyCode::Char('j')) {
                    *selected_link = selected_link
                        .saturating_add(1)
                        .min(answer.hunk_ids.len().saturating_sub(1));
                    return true;
                }
                if plain_key(event, KeyCode::Char('k')) {
                    *selected_link = selected_link.saturating_sub(1);
                    return true;
                }
                if plain_key(event, KeyCode::Enter)
                    && let Some(id) = answer.hunk_ids.get(*selected_link).cloned()
                {
                    self.ask = AskState::Closed;
                    self.open_hunk(id);
                    return true;
                }
                false
            }
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
            kind: ActiveRequestKind::Generate,
        });
        self.pending_task = Some(ReviewCodexTask {
            id,
            request: ReviewCodexRequest::Generate(request),
            cancellation,
        });
        self.failure = None;
    }

    fn start_question(&mut self) {
        let AskState::Editing { question } = &self.ask else {
            return;
        };
        let question = question.trim().to_owned();
        if question.is_empty() {
            return;
        }
        let Some(cached) = self.ready() else {
            self.ask = AskState::Closed;
            return;
        };
        let request = AskRequest {
            review_request: cached.request.clone(),
            review: cached.result.clone(),
            selected_hunk_id: self.active_hunk_id.clone(),
            question: question.clone(),
        };
        let id = self.next_id();
        let cancellation = CancellationHandle::default();
        self.active_request = Some(ActiveRequest {
            id,
            cancellation: cancellation.clone(),
            kind: ActiveRequestKind::Ask,
        });
        self.pending_task = Some(ReviewCodexTask {
            id,
            request: ReviewCodexRequest::Ask(request),
            cancellation,
        });
        self.ask = AskState::Running { question };
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
            if active.kind == ActiveRequestKind::Ask {
                self.ask = AskState::Closed;
            }
            return true;
        }
        match result.outcome {
            ReviewCodexOutcome::Generated(review) if active.kind == ActiveRequestKind::Generate => {
                let Some(request) = ReviewRequest::from_snapshot(&self.model.snapshot) else {
                    return true;
                };
                self.cached = Some(CachedReview {
                    request,
                    result: review,
                });
                self.failure = None;
                self.selected_stop = 0;
                self.visited.clear();
                self.open_selected_stop();
            }
            ReviewCodexOutcome::Answered(answer) if active.kind == ActiveRequestKind::Ask => {
                let question = match std::mem::take(&mut self.ask) {
                    AskState::Running { question } => question,
                    _ => String::new(),
                };
                self.ask = AskState::Answered {
                    question,
                    answer,
                    selected_link: 0,
                };
            }
            ReviewCodexOutcome::Failed(error) => {
                if active.kind == ActiveRequestKind::Ask {
                    self.ask = AskState::Closed;
                }
                self.failure = Some(error);
            }
            ReviewCodexOutcome::Cancelled => {
                if active.kind == ActiveRequestKind::Ask {
                    self.ask = AskState::Closed;
                }
            }
            ReviewCodexOutcome::Generated(_) | ReviewCodexOutcome::Answered(_) => {}
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
            && let Some(id) = self.pending_hunk_id.take()
        {
            self.visited.insert(id);
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

    #[must_use]
    pub(crate) fn captures_global_input(&self) -> bool {
        !matches!(self.ask, AskState::Closed)
    }

    pub(crate) fn help_rows() -> Vec<(String, &'static str)> {
        vec![
            ("j / k".to_owned(), "Select review stop"),
            ("Enter".to_owned(), "Open or generate"),
            ("n / p".to_owned(), "Next / previous attention stop"),
            ("/".to_owned(), "Ask the diff"),
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
