//! Checkout commit history, file selection, and shared review rendering.

mod document;
mod requests;
mod view;

use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
};

use crate::diff::{
    DiffViewMode, FramePreparation, Message, Renderer, RendererEvent, ReviewDocument,
    ReviewHunkSegment, ReviewHunkSet, ReviewRender, ReviewSelection, ReviewState,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use diffo_core::{CheckoutHistory, Commit, CommitFile, RepositoryQueryId, RepositorySnapshot};
use diffo_ui::{
    PaneSplit,
    file_picker::{FilePicker, Navigation as PickerNavigation, Outcome as PickerOutcome},
    text_view::TextSurface,
};
use ratatui::{Frame, layout::Rect, text::Line};

use document::{
    commit_title, selection_commit_id, selection_target, snapshot_head, split_file_patches,
};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum HistoryTarget {
    File(PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HistoryRequest {
    Commits {
        query_id: RepositoryQueryId,
    },
    Patch {
        query_id: RepositoryQueryId,
        commit_id: String,
    },
    File {
        query_id: RepositoryQueryId,
        commit_id: String,
        path: PathBuf,
        old_path: Option<PathBuf>,
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
    commit_picker: FilePicker<String>,
    file_picker: FilePicker<HistoryTarget>,
    files: Vec<CommitFile>,
    pending_files: Option<Vec<CommitFile>>,
    hunks: Option<ReviewHunkSet>,
    pending_hunks: Option<ReviewHunkSet>,
    file_patches: HashMap<PathBuf, Arc<str>>,
    document: Option<ReviewDocument>,
    pending_document: Option<ReviewDocument>,
    selection: Option<ReviewSelection>,
    pending_selection: Option<ReviewSelection>,
    reviewer: Renderer,
    queued: VecDeque<HistoryRequest>,
    next_id: u64,
    latest_history: u64,
    latest_patch: u64,
    latest_file: u64,
    history_pending: bool,
    patch_pending: bool,
    file_pending: bool,
    review: ReviewState,
    pending_mode: Option<DiffViewMode>,
}

impl HistoryActivity {
    #[must_use]
    pub fn new(snapshot: &RepositorySnapshot) -> Self {
        let mut activity = Self {
            head_commit: snapshot_head(snapshot),
            commits: Vec::new(),
            pending_commits: None,
            commit_picker: FilePicker::default(),
            file_picker: FilePicker::default(),
            files: Vec::new(),
            pending_files: None,
            hunks: None,
            pending_hunks: None,
            file_patches: HashMap::new(),
            document: None,
            pending_document: None,
            selection: None,
            pending_selection: None,
            reviewer: Renderer::new(),
            queued: VecDeque::new(),
            next_id: 0,
            latest_history: 0,
            latest_patch: 0,
            latest_file: 0,
            history_pending: false,
            patch_pending: false,
            file_pending: false,
            review: ReviewState::default(),
            pending_mode: None,
        };
        activity.request_history();
        activity
    }

    pub fn repository_changed(&mut self, snapshot: &RepositorySnapshot) {
        let head = snapshot_head(snapshot);
        if head == self.head_commit {
            return;
        }
        self.head_commit = head;
        self.pending_commits = None;
        self.pending_selection = None;
        self.pending_document = None;
        self.pending_files = None;
        self.pending_hunks = None;
        self.pending_mode = None;
        self.patch_pending = false;
        self.file_pending = false;
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
            self.commits.clear();
            self.pending_commits = None;
            self.files.clear();
            self.pending_files = None;
            self.hunks = None;
            self.pending_hunks = None;
            self.file_patches.clear();
            self.document = None;
            self.pending_document = None;
            self.selection = None;
            self.pending_selection = None;
            self.review.set_viewport(0, 0);
            return true;
        }
        let target = self
            .selection
            .as_ref()
            .and_then(selection_commit_id)
            .filter(|selected| history.commits.iter().any(|commit| commit.id == *selected))
            .map_or_else(|| history.commits[0].id.clone(), str::to_owned);
        if self
            .selection
            .as_ref()
            .and_then(selection_commit_id)
            .is_some_and(|selected| selected == target)
        {
            self.commits = history.commits;
            return true;
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
        commit_id: &str,
        patch: String,
        files: Vec<CommitFile>,
    ) -> bool {
        if query_id.0 != self.latest_patch
            || self
                .pending_selection
                .as_ref()
                .and_then(selection_commit_id)
                != Some(commit_id)
        {
            return false;
        }
        let Some(summary) = self.commit_summary(commit_id).map(str::to_owned) else {
            return false;
        };
        self.patch_pending = false;
        let patch = Arc::<str>::from(patch);
        let file_patches = split_file_patches(&patch);
        let mut segments = files
            .iter()
            .zip(file_patches)
            .map(|(file, patch)| ReviewHunkSegment {
                selection: ReviewSelection::HistoryFile {
                    commit_id: commit_id.to_owned(),
                    path: file.path.clone(),
                },
                patch,
                mark_conflicts: false,
            })
            .collect::<Vec<_>>();
        if segments.is_empty() && !patch.is_empty() {
            segments.push(ReviewHunkSegment {
                selection: ReviewSelection::CompleteChange(commit_id.to_owned()),
                patch: Arc::clone(&patch),
                mark_conflicts: false,
            });
        }
        let hunks = ReviewHunkSet {
            id: format!("commit:{commit_id}"),
            title: commit_title(commit_id, &summary),
            segments: Arc::from(segments),
        };
        let selection = files.first().map_or_else(
            || ReviewSelection::CompleteChange(commit_id.to_owned()),
            |file| ReviewSelection::HistoryFile {
                commit_id: commit_id.to_owned(),
                path: file.path.clone(),
            },
        );
        let title = files
            .first()
            .map_or_else(|| commit_title(commit_id, &summary), view::file_title);
        self.pending_document = Some(ReviewDocument {
            selection: selection.clone(),
            title,
            patch: Arc::from(""),
            mark_conflicts: false,
            hunks: hunks.clone(),
        });
        self.pending_selection = Some(selection);
        self.pending_hunks = Some(hunks);
        self.pending_mode = Some(DiffViewMode::Hunk);
        self.pending_files = Some(files);
        true
    }

    pub fn patch_failed(&mut self, query_id: RepositoryQueryId, commit_id: &str) -> bool {
        if query_id.0 != self.latest_patch
            || self
                .pending_selection
                .as_ref()
                .and_then(selection_commit_id)
                != Some(commit_id)
        {
            return false;
        }
        self.patch_pending = false;
        self.pending_commits = None;
        self.pending_selection = None;
        self.pending_document = None;
        self.pending_files = None;
        self.pending_hunks = None;
        self.pending_mode = None;
        true
    }

    pub fn accept_file(
        &mut self,
        query_id: RepositoryQueryId,
        commit_id: &str,
        path: PathBuf,
        contents: String,
    ) -> bool {
        let expected = ReviewSelection::HistoryFile {
            commit_id: commit_id.to_owned(),
            path: path.clone(),
        };
        if query_id.0 != self.latest_file || self.pending_selection.as_ref() != Some(&expected) {
            return false;
        }
        let Some(file) = self
            .pending_files
            .as_deref()
            .unwrap_or(&self.files)
            .iter()
            .find(|file| file.path == path)
        else {
            return false;
        };
        self.file_pending = false;
        let contents = Arc::<str>::from(contents);
        self.file_patches.insert(path, Arc::clone(&contents));
        let Some(hunks) = self.pending_hunks.as_ref().or(self.hunks.as_ref()).cloned() else {
            return false;
        };
        self.pending_document = Some(ReviewDocument {
            selection: expected,
            title: view::file_title(file),
            patch: contents,
            mark_conflicts: false,
            hunks,
        });
        true
    }

    pub fn file_failed(
        &mut self,
        query_id: RepositoryQueryId,
        commit_id: &str,
        path: &std::path::Path,
    ) -> bool {
        let expected = ReviewSelection::HistoryFile {
            commit_id: commit_id.to_owned(),
            path: path.to_path_buf(),
        };
        if query_id.0 != self.latest_file || self.pending_selection.as_ref() != Some(&expected) {
            return false;
        }
        self.file_pending = false;
        self.pending_selection = None;
        self.pending_document = None;
        self.pending_mode = None;
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

    fn promote_ready_selection(&mut self) {
        let Some(pending) = self.pending_selection.as_ref() else {
            return;
        };
        if self.reviewer.displayed_review_selection() != Some(pending)
            || self
                .pending_mode
                .is_some_and(|mode| self.reviewer.displayed_review_mode() != Some(mode))
        {
            return;
        }
        let previous_commit = self.selection.as_ref().and_then(selection_commit_id);
        let next_commit = self
            .pending_selection
            .as_ref()
            .and_then(selection_commit_id);
        let changed_commit = previous_commit != next_commit;
        self.selection = self.pending_selection.take();
        self.document = self.pending_document.take();
        if let Some(mode) = self.pending_mode.take() {
            self.review.diff_view_mode = mode;
        }
        if let Some(commits) = self.pending_commits.take() {
            self.commits = commits;
        }
        if let Some(files) = self.pending_files.take() {
            self.files = files;
        }
        if let Some(hunks) = self.pending_hunks.take() {
            self.hunks = Some(hunks);
        }
        if changed_commit {
            self.file_patches.clear();
        }
    }

    pub fn prepare_frame(&mut self, area: Rect, split: PaneSplit) -> FramePreparation {
        let areas = view::areas(area, split);
        let requested = self.pending_document.as_ref().or(self.document.as_ref());
        let requested_mode = self
            .pending_document
            .as_ref()
            .map_or(self.review.diff_view_mode, |_| {
                self.pending_mode.unwrap_or(self.review.diff_view_mode)
            });
        let mut preparation = self.reviewer.prepare_review(
            requested,
            areas.review,
            false,
            requested_mode,
            self.review.diff_scroll,
            self.review.diff_horizontal_scroll,
        );
        self.review.apply_preparation(&preparation);
        self.promote_ready_selection();
        self.prepare_pickers(areas, split);
        preparation.preparing |= self.history_pending || self.patch_pending || self.file_pending;
        let (requested_commit, selected_commit, displayed_commit) = self.document_commits();
        preparation.requested_history_commit = requested_commit;
        preparation.selected_history_commit = selected_commit;
        preparation.displayed_history_commit = displayed_commit;
        let (requested_file, selected_file, displayed_file) = self.document_files();
        preparation.requested_history_file = requested_file;
        preparation.selected_history_file = selected_file;
        preparation.displayed_history_file = displayed_file;
        if let Some(surface) = preparation.text_surface.as_mut() {
            surface.surface = TextSurface::History;
        }
        preparation
    }

    pub fn prepare_full_screen(&mut self, area: Rect) -> FramePreparation {
        let requested = self.pending_document.as_ref().or(self.document.as_ref());
        let requested_mode = self
            .pending_document
            .as_ref()
            .map_or(self.review.diff_view_mode, |_| {
                self.pending_mode.unwrap_or(self.review.diff_view_mode)
            });
        let mut preparation = self.reviewer.prepare_review(
            requested,
            area,
            true,
            requested_mode,
            self.review.diff_scroll,
            self.review.diff_horizontal_scroll,
        );
        self.review.apply_preparation(&preparation);
        self.promote_ready_selection();
        preparation.preparing |= self.history_pending || self.patch_pending || self.file_pending;
        let (requested_commit, selected_commit, displayed_commit) = self.document_commits();
        preparation.requested_history_commit = requested_commit;
        preparation.selected_history_commit = selected_commit;
        preparation.displayed_history_commit = displayed_commit;
        let (requested_file, selected_file, displayed_file) = self.document_files();
        preparation.requested_history_file = requested_file;
        preparation.selected_history_file = selected_file;
        preparation.displayed_history_file = displayed_file;
        if let Some(surface) = preparation.text_surface.as_mut() {
            surface.surface = TextSurface::History;
        }
        preparation
    }

    fn prepare_pickers(&mut self, areas: view::HistoryAreas, split: PaneSplit) {
        let commit = self
            .selection
            .as_ref()
            .and_then(|selection| match selection {
                ReviewSelection::File(_) => None,
                ReviewSelection::HistoryFile { commit_id, .. }
                | ReviewSelection::CompleteChange(commit_id) => Some(commit_id),
            });
        let target = self.selection.as_ref().and_then(selection_target);
        self.commit_picker.prepare(
            areas.commits,
            view::commit_document(
                &self.commits,
                split.border_style(),
                self.history_pending || (self.commits.is_empty() && self.pending_commits.is_some()),
            ),
            commit,
        );
        self.file_picker.prepare(
            areas.files,
            view::file_document(&self.files, split.border_style()),
            target.as_ref(),
        );
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, split: PaneSplit) {
        let areas = view::areas(area, split);
        self.commit_picker.render(frame, self.selection.is_some());
        self.file_picker.render(frame, self.selection.is_some());
        self.reviewer.render_review(
            frame,
            areas.review,
            ReviewRender {
                mode: self.review.diff_view_mode,
                vertical: self.review.diff_scroll,
                horizontal: self.review.diff_horizontal_scroll,
                border_style: split.border_style(),
                trailing_title: "",
                has_selection: self.selection.is_some(),
                empty_title: "Commit Diff",
            },
        );
    }

    pub fn render_full_screen(&mut self, frame: &mut Frame, area: Rect) {
        self.reviewer.render_review_full_screen(
            frame,
            area,
            self.review.diff_view_mode,
            self.review.diff_scroll,
            self.review.diff_horizontal_scroll,
        );
    }

    #[must_use]
    pub fn full_screen_title(&self) -> Option<Line<'static>> {
        self.reviewer.full_screen_title()
    }

    pub fn handle_event(
        &mut self,
        event: &Event,
        area: Rect,
        split: PaneSplit,
    ) -> Option<HistoryEvent> {
        if let Event::Key(key) = event {
            if key.kind != KeyEventKind::Press || key.modifiers != KeyModifiers::NONE {
                return None;
            }
            let changed = match key.code {
                KeyCode::Char('j') => Some(self.navigate_commits(PickerNavigation::Previous)),
                KeyCode::Char('k') => Some(self.navigate_commits(PickerNavigation::Next)),
                KeyCode::Char('h') => Some(self.navigate_files(PickerNavigation::Previous)),
                KeyCode::Char('l') => Some(self.navigate_files(PickerNavigation::Next)),
                _ => None,
            };
            if let Some(changed) = changed {
                return changed.then_some(HistoryEvent::Consumed);
            }
        }
        if let Some(outcome) = self.commit_picker.handle_event(event, area) {
            return self
                .handle_commit_outcome(outcome)
                .then_some(HistoryEvent::Consumed);
        }
        if let Some(outcome) = self.file_picker.handle_event(event, area) {
            return self
                .handle_file_outcome(outcome)
                .then_some(HistoryEvent::Consumed);
        }
        self.handle_review_event(event, view::areas(area, split).review)
    }

    pub fn handle_full_screen_event(&mut self, event: &Event, area: Rect) -> Option<HistoryEvent> {
        self.handle_review_event(event, area)
    }

    fn navigate_commits(&mut self, navigation: PickerNavigation) -> bool {
        self.commit_picker
            .navigate(navigation)
            .is_some_and(|outcome| self.handle_commit_outcome(outcome))
    }

    fn navigate_files(&mut self, navigation: PickerNavigation) -> bool {
        self.file_picker
            .navigate(navigation)
            .is_some_and(|outcome| self.handle_file_outcome(outcome))
    }

    fn handle_commit_outcome(&mut self, outcome: PickerOutcome<String>) -> bool {
        match outcome {
            PickerOutcome::Selected(commit_id) | PickerOutcome::Activated(commit_id) => {
                if self.selection.as_ref().and_then(selection_commit_id) != Some(&commit_id) {
                    self.request_patch(commit_id);
                }
                true
            }
            PickerOutcome::Consumed => true,
            PickerOutcome::RowAction(_)
            | PickerOutcome::PanelAction
            | PickerOutcome::CopyPath { .. }
            | PickerOutcome::DestructiveAction(_) => false,
        }
    }

    fn handle_file_outcome(&mut self, outcome: PickerOutcome<HistoryTarget>) -> bool {
        let target = match outcome {
            PickerOutcome::Selected(target) | PickerOutcome::Activated(target) => target,
            PickerOutcome::Consumed => return true,
            PickerOutcome::RowAction(_)
            | PickerOutcome::PanelAction
            | PickerOutcome::CopyPath { .. }
            | PickerOutcome::DestructiveAction(_) => return false,
        };
        let Some(commit_id) = self
            .selection
            .as_ref()
            .and_then(selection_commit_id)
            .map(str::to_owned)
        else {
            return false;
        };
        let HistoryTarget::File(path) = target;
        self.select_file(commit_id, &path, self.review.diff_view_mode)
    }

    fn toggle_review_mode(&mut self) -> bool {
        let mode = self
            .pending_mode
            .unwrap_or(self.review.diff_view_mode)
            .toggled();
        let Some(ReviewSelection::HistoryFile { commit_id, path }) = self
            .pending_selection
            .as_ref()
            .or(self.selection.as_ref())
            .cloned()
        else {
            return false;
        };
        self.select_file(commit_id, &path, mode)
    }

    fn select_file(
        &mut self,
        commit_id: String,
        path: &std::path::Path,
        mode: DiffViewMode,
    ) -> bool {
        let Some(file) = self
            .pending_files
            .as_deref()
            .unwrap_or(&self.files)
            .iter()
            .find(|file| file.path == path)
            .cloned()
        else {
            return false;
        };
        let selection = ReviewSelection::HistoryFile {
            commit_id: commit_id.clone(),
            path: path.to_path_buf(),
        };
        self.pending_mode = Some(mode);
        if mode != DiffViewMode::Hunk && !self.file_patches.contains_key(path) {
            self.request_file(commit_id, &file);
            return true;
        }
        let Some(hunks) = self.pending_hunks.as_ref().or(self.hunks.as_ref()).cloned() else {
            return false;
        };
        self.pending_selection = Some(selection.clone());
        self.pending_document = Some(ReviewDocument {
            selection,
            title: view::file_title(&file),
            patch: self
                .file_patches
                .get(path)
                .cloned()
                .unwrap_or_else(|| Arc::from("")),
            mark_conflicts: false,
            hunks,
        });
        true
    }

    fn handle_review_event(&mut self, event: &Event, area: Rect) -> Option<HistoryEvent> {
        match self.reviewer.map_review_event(event, &self.review, area)? {
            RendererEvent::Consumed | RendererEvent::Message(Message::JumpDiffToPosition(_)) => {
                Some(HistoryEvent::Consumed)
            }
            RendererEvent::Message(Message::ToggleDiffView) => {
                self.toggle_review_mode().then_some(HistoryEvent::Consumed)
            }
            RendererEvent::Message(message) => self
                .review
                .update(&message)
                .then_some(HistoryEvent::Consumed),
            RendererEvent::CopyPath { .. } => None,
        }
    }

    pub fn take_request(&mut self) -> Option<HistoryRequest> {
        self.queued.pop_front()
    }
}

#[cfg(test)]
mod tests;
