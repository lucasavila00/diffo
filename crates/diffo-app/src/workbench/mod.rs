//! Activity composition, global input routing, and command lifecycle.

use std::{
    collections::{HashMap, VecDeque},
    time::Instant,
};

use crate::diff::{DiffViewMode, Effect, Message, Model, ToastKind, ToastQueue, update};
use crate::diff::{FramePreparation, Renderer, RendererEvent, toast_at_position};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_core::{
    ApplicationCommandId, GitPrompt, PromptId, RepositoryAction, RepositoryQueryId,
    RepositorySnapshot,
};
use diffo_ui::command_palette::{Command, CommandId};
use diffo_ui::text_view::{TextRenderMode, TextSurfacePreparation};
use diffo_ui::{PaneSplit, tool_areas};
use ratatui::{Frame, layout::Rect};

use crate::explorer::{ExplorerActivity, ExplorerEvent, ExplorerOutcome};
use crate::history::{HistoryActivity, HistoryEvent, HistoryRequest};
mod activity_bar;
mod ai_commit;
mod application_update;
mod bindings;
mod checkout_picker;
mod command_queue;
mod create_branch;
mod delete_branch;
mod error_dialog;
mod full_screen;
mod help;
mod history;
mod merge;
mod modal;
mod pending_scroll;
mod presentation;
mod prompt;
mod quick_open;
mod repository_update;
mod sync_remote;
mod types;

use bindings::GlobalAction;
use error_dialog::ErrorDialog;
use modal::Modal;
use pending_scroll::PendingScroll;
use presentation::PresentationState;
use prompt::{ConfirmChoice, PromptModal, prompt_button_style, prompt_layout, render_prompt};

pub use activity_bar::{
    ACTIVITY_BAR_WIDTH, WorkbenchAreas, activity_at_position, render_activity_bar, workbench_areas,
};
pub use ai_commit::*;
pub use application_update::UpdateOutcome;
pub(crate) use command_queue::CommandIntent;
pub use command_queue::{
    ApplicationAction, ApplicationCommand, CommandQueue, CommandResult, CommandState,
};
use types::WorkbenchCommand;
pub use types::{Activity, PromptResponse, WorkbenchEffect, WorkbenchTask, WorkbenchTaskResult};

pub struct Workbench {
    active: Activity,
    diff: DiffActivity,
    explorer: ExplorerActivity,
    history: HistoryActivity,
    selected_review_mode: Option<DiffViewMode>,
    pane_split: PaneSplit,
    toasts: ToastQueue,
    toast_deadlines: HashMap<u64, Instant>,
    pending_errors: VecDeque<ErrorDialog>,
    commands: CommandQueue,
    repository_generation: u64,
    command_progress: CommandProgressState,
    command_animation_tick: usize,
    should_quit: bool,
    full_screen: bool,
    full_screen_pending: bool,
    modal: Option<Modal>,
    last_prompt_id: Option<PromptId>,
    pending_branch_query: Option<RepositoryQueryId>,
    pending_merge_query: Option<RepositoryQueryId>,
    pending_sync_remote_query: Option<RepositoryQueryId>,
    next_query_id: u64,
    presentation: PresentationState,
}

struct DiffActivity {
    model: Model,
    renderer: Renderer,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum CommandProgressState {
    #[default]
    Hidden,
    Waiting {
        command_id: ApplicationCommandId,
        reveal_at: Instant,
    },
    Visible {
        command_id: ApplicationCommandId,
    },
}

impl CommandProgressState {
    const fn command_id(self) -> Option<ApplicationCommandId> {
        match self {
            Self::Hidden => None,
            Self::Waiting { command_id, .. } | Self::Visible { command_id } => Some(command_id),
        }
    }

    const fn is_visible(self) -> bool {
        matches!(self, Self::Visible { .. })
    }
}

const FETCH_COMMAND: CommandId = CommandId::new("git.fetch");
const SYNC_COMMAND: CommandId = CommandId::new("git.sync");
const CHECKOUT_COMMAND: CommandId = CommandId::new("git.checkout_to");
const UPDATE_COMMAND: CommandId = CommandId::new("application.update");

const SHARED_COMMANDS: [Command; 7] = [
    Command {
        id: FETCH_COMMAND,
        label: "Git: Fetch",
    },
    Command {
        id: SYNC_COMMAND,
        label: "Git: Sync",
    },
    Command {
        id: CHECKOUT_COMMAND,
        label: "Git: Checkout to...",
    },
    create_branch::CREATE_BRANCH_PALETTE_COMMAND,
    create_branch::CREATE_BRANCH_FROM_PALETTE_COMMAND,
    delete_branch::DELETE_BRANCH_PALETTE_COMMAND,
    Command {
        id: UPDATE_COMMAND,
        label: "Application: Update Diffo",
    },
];

trait Tool {
    fn handle_event(
        &mut self,
        event: &Event,
        area: Rect,
        split: PaneSplit,
    ) -> Option<WorkbenchCommand>;
    fn prepare_frame(&mut self, area: Rect, split: PaneSplit) -> FramePreparation;
    fn render(&mut self, frame: &mut Frame, area: Rect, split: PaneSplit);
    fn is_preparing(&self) -> bool;
    fn captures_global_input(&self) -> bool {
        false
    }
    fn commands(&self) -> &'static [Command] {
        &[]
    }
    fn help_rows(&self) -> Vec<(String, &'static str)>;
    fn dismiss_popover(&mut self) {}
    fn execute_command(&mut self, _command: CommandId) -> bool {
        false
    }
}

impl Workbench {
    #[must_use]
    pub fn new(snapshot: RepositorySnapshot) -> Self {
        let explorer = ExplorerActivity::new(&snapshot);
        let history = HistoryActivity::new(&snapshot);
        Self {
            active: Activity::Diff,
            diff: DiffActivity {
                model: Model::new(snapshot),
                renderer: Renderer::new(),
            },
            explorer,
            history,
            selected_review_mode: None,
            pane_split: PaneSplit::default(),
            toasts: ToastQueue::new(),
            toast_deadlines: HashMap::new(),
            pending_errors: VecDeque::new(),
            commands: CommandQueue::new(),
            repository_generation: 0,
            command_progress: CommandProgressState::Hidden,
            command_animation_tick: 0,
            should_quit: false,
            full_screen: false,
            full_screen_pending: false,
            modal: None,
            last_prompt_id: None,
            pending_branch_query: None,
            pending_merge_query: None,
            pending_sync_remote_query: None,
            next_query_id: 1,
            presentation: PresentationState::new(),
        }
    }

    #[must_use]
    pub fn should_quit(&self) -> bool {
        self.should_quit || self.diff.model.should_quit
    }

    #[must_use]
    pub const fn active(&self) -> Activity {
        self.active
    }

    #[must_use]
    pub const fn diff_model(&self) -> &Model {
        &self.diff.model
    }

    pub fn diff_model_mut(&mut self) -> &mut Model {
        &mut self.diff.model
    }

    #[must_use]
    pub fn secret_prompt_open(&self) -> bool {
        matches!(
            self.modal,
            Some(Modal::GitPrompt(ref modal)) if matches!(modal.prompt, GitPrompt::Secret { .. })
        )
    }

    #[must_use]
    pub fn modal_trace_label(&self) -> Option<&'static str> {
        match self.modal {
            Some(Modal::CommitEditor) => Some("CommitEditor"),
            Some(Modal::Error(_)) => Some("Error"),
            _ => None,
        }
    }

    pub fn tick(&mut self, now: Instant) {
        if self.expire_toasts(now) {
            self.request_redraw();
        }
        let progress_before = self.command_progress;
        if let CommandProgressState::Waiting {
            command_id,
            reveal_at,
        } = self.command_progress
            && self
                .commands
                .active()
                .is_some_and(|command| command.id == command_id)
            && now >= reveal_at
        {
            self.command_progress = CommandProgressState::Visible { command_id };
        }
        if self.command_progress.is_visible() {
            self.command_animation_tick = self.command_animation_tick.wrapping_add(1);
            self.request_redraw();
        } else {
            self.command_animation_tick = 0;
        }
        if self.command_progress != progress_before {
            self.request_redraw();
        }
    }

    #[must_use]
    pub fn has_active_command(&self) -> bool {
        self.commands.active().is_some()
    }

    #[must_use]
    pub fn active_command_id(&self) -> Option<ApplicationCommandId> {
        self.commands.active().map(|command| command.id)
    }

    pub fn cancel_application_command(&mut self, id: ApplicationCommandId) -> bool {
        self.commands.cancel(id)
    }

    #[must_use]
    pub const fn full_screen(&self) -> bool {
        self.full_screen
    }

    pub fn handle_events(&mut self, events: &[Event], area: Rect) -> Vec<WorkbenchEffect> {
        let mut scroll = PendingScroll::default();
        let mut effects = Vec::new();
        for event in events {
            let Some(command) = self.handle_event(event, area) else {
                continue;
            };
            match command {
                WorkbenchCommand::Diff(message) if scroll.push(&message) => {}
                WorkbenchCommand::Diff(message) => {
                    scroll.flush(self);
                    if let Some(effect) = self.update_diff(message) {
                        effects.push(effect);
                    }
                }
                WorkbenchCommand::Effect(effect) => {
                    scroll.flush(self);
                    effects.push(effect);
                }
                WorkbenchCommand::Redraw => {
                    scroll.flush(self);
                    self.request_redraw();
                }
            }
        }
        scroll.flush(self);
        effects
    }

    fn handle_event(&mut self, event: &Event, area: Rect) -> Option<WorkbenchCommand> {
        let content = workbench_areas(area).content;
        if self.cancel_clicked_command(event, content) {
            return Some(WorkbenchCommand::Redraw);
        }
        if self.modal.is_some() {
            return self.handle_modal_event(event, area);
        }
        if !self.full_screen && self.select_activity(event, area) {
            return Some(WorkbenchCommand::Redraw);
        }
        let tool_captures_global_input = self.active_tool_captures_global_input();
        if self.full_screen
            && !tool_captures_global_input
            && bindings::action(event) == Some(GlobalAction::Sync)
        {
            return self.execute_sync();
        }
        if self.full_screen {
            return self.handle_full_screen_event(event, area);
        }
        if !tool_captures_global_input && self.handle_overlay_click(event, content) {
            return Some(WorkbenchCommand::Redraw);
        }
        if !tool_captures_global_input && self.request_full_screen(event, area) {
            return Some(WorkbenchCommand::Redraw);
        }
        if !tool_captures_global_input
            && let Event::Mouse(mouse) = event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && let Some(control) = crate::diff::footer_control_at_position(
                &self.diff.model,
                tool_areas(content).status,
                mouse.column,
                mouse.row,
            )
        {
            match control {
                crate::diff::FooterControl::Commands => self.open_active_palette(),
                crate::diff::FooterControl::Help => self.set_modal(Modal::Help),
                crate::diff::FooterControl::Sync => return self.execute_sync(),
            }
            return Some(WorkbenchCommand::Redraw);
        }
        let pane_area = tool_areas(content).content;
        if !tool_captures_global_input && let Event::Mouse(mouse) = event {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left)
                    if self
                        .pane_split
                        .contains_seam(pane_area, mouse.column, mouse.row) =>
                {
                    self.pane_split.begin_drag();
                    self.sync_diff_pane_state();
                    return Some(WorkbenchCommand::Redraw);
                }
                MouseEventKind::Drag(MouseButton::Left) if self.pane_split.is_dragging() => {
                    self.pane_split.drag_to(pane_area, mouse.column);
                    self.sync_diff_pane_state();
                    return Some(WorkbenchCommand::Redraw);
                }
                MouseEventKind::Up(MouseButton::Left) if self.pane_split.is_dragging() => {
                    self.pane_split.end_drag();
                    self.sync_diff_pane_state();
                    return Some(WorkbenchCommand::Redraw);
                }
                _ => {}
            }
        }
        if !tool_captures_global_input && let Some(action) = bindings::action(event) {
            match action {
                GlobalAction::OpenCommandPalette => self.open_active_palette(),
                GlobalAction::ToggleHelp => self.set_modal(Modal::Help),
                GlobalAction::Sync => return self.execute_sync(),
                GlobalAction::QuickOpen => self.open_quick_open(),
            }
            return Some(WorkbenchCommand::Redraw);
        }
        if self.active != Activity::Diff
            && !tool_captures_global_input
            && let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && (matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                || (key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL)))
        {
            self.should_quit = true;
            return None;
        }
        self.handle_active_tool_event(event, content)
    }

    fn active_tool_captures_global_input(&self) -> bool {
        match self.active {
            Activity::Diff => self.diff.captures_global_input(),
            Activity::Explorer => self.explorer.captures_global_input(),
            Activity::History => self.history.captures_global_input(),
        }
    }

    fn handle_active_tool_event(
        &mut self,
        event: &Event,
        content: Rect,
    ) -> Option<WorkbenchCommand> {
        match self.active {
            Activity::Diff => self.diff.handle_event(event, content, self.pane_split),
            Activity::Explorer => {
                Tool::handle_event(&mut self.explorer, event, content, self.pane_split)
            }
            Activity::History => {
                let mode = self.history.review_mode();
                let command =
                    Tool::handle_event(&mut self.history, event, content, self.pane_split);
                if self.history.review_mode() != mode {
                    let mode = self.history.review_mode();
                    self.selected_review_mode = Some(mode);
                    self.diff.model.review.diff_view_mode = mode;
                }
                command
            }
        }
    }

    fn select_activity(&mut self, event: &Event, area: Rect) -> bool {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && key.code == KeyCode::Tab
            && key.modifiers == KeyModifiers::NONE
        {
            self.dismiss_active_popover();
            self.active = self.active.next();
            self.sync_review_mode_for_active_activity();
            return true;
        }
        if let Event::Mouse(mouse) = event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && let Some(activity) = activity_at_position(area, mouse.column, mouse.row)
        {
            if activity == self.active {
                return false;
            }
            self.dismiss_active_popover();
            self.active = activity;
            self.sync_review_mode_for_active_activity();
            return true;
        }
        false
    }

    fn sync_review_mode_for_active_activity(&mut self) {
        let Some(mode) = self.selected_review_mode else {
            return;
        };
        match self.active {
            Activity::Diff => {
                self.diff.model.review.diff_view_mode = mode;
            }
            Activity::History => {
                self.history.set_review_mode(mode);
            }
            Activity::Explorer => {}
        }
    }

    fn dismiss_clicked_toast(&mut self, event: &Event, area: Rect) -> bool {
        let Event::Mouse(mouse) = event else {
            return false;
        };
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return false;
        }
        let Some(id) = toast_at_position(self.toasts.as_slice(), area, mouse.column, mouse.row)
        else {
            return false;
        };
        self.toasts.dismiss(id);
        self.toast_deadlines.remove(&id);
        true
    }

    fn handle_overlay_click(&mut self, event: &Event, area: Rect) -> bool {
        self.dismiss_clicked_toast(event, area)
    }

    fn sync_diff_pane_state(&mut self) {
        self.diff.model.file_pane_percent = self.pane_split.percent();
        self.diff.model.resizing_file_pane = self.pane_split.is_dragging();
    }

    fn dismiss_active_popover(&mut self) {
        match self.active {
            Activity::Diff => self.diff.dismiss_popover(),
            Activity::Explorer => self.explorer.dismiss_popover(),
            Activity::History => self.history.dismiss_popover(),
        }
    }

    pub fn accept_task_result(&mut self, result: WorkbenchTaskResult) {
        match result {
            WorkbenchTaskResult::Explorer(outcome) => {
                let paths_refreshed =
                    matches!(&outcome, ExplorerOutcome::Paths { result: Ok(_), .. });
                let (error, changed) = self.explorer.accept(outcome);
                if let Some((title, detail)) = error {
                    self.show_error(title, detail);
                }
                if changed && self.active == Activity::Explorer {
                    self.request_redraw();
                }
                if paths_refreshed && matches!(self.modal, Some(Modal::QuickOpen(_))) {
                    self.explorer.request_quick_open_paths();
                }
                if changed || paths_refreshed {
                    self.refresh_quick_open();
                }
            }
        }
    }

    fn update_diff(&mut self, message: Message) -> Option<WorkbenchEffect> {
        if message == Message::ExecuteAiCommit {
            self.request_ai_commit();
            return None;
        }
        if self.commands.has_work() && self.enqueue_followup_intent(&message) {
            return None;
        }
        if message == Message::FocusCommitInput {
            if self.diff.model.ai_commit_pending() {
                return None;
            }
            self.set_modal(Modal::CommitEditor);
            return None;
        }
        if message == Message::BlurCommitInput {
            self.close_modal();
            return None;
        }
        let review_mode_changed = message == Message::ToggleDiffView;
        let preparation_owned = matches!(
            &message,
            Message::SelectFile(_)
                | Message::ScrollDiffUp
                | Message::ScrollDiffDown
                | Message::ScrollDiffPageUp(_)
                | Message::ScrollDiffPageDown(_)
                | Message::ScrollDiffVerticalBy(_)
                | Message::SetDiffScroll(_)
                | Message::JumpDiffToPosition(_)
                | Message::SetDiffHorizontalScroll(_)
                | Message::ScrollDiffLeft
                | Message::ScrollDiffRight
                | Message::ScrollDiffHorizontalBy(_)
                | Message::JumpToPreviousChange
                | Message::JumpToNextChange
                | Message::ToggleDiffView
        );
        let model_before = self.diff.model.clone();
        let commit_submission = message == Message::ExecuteCommit;
        match &message {
            Message::SnapshotLoaded(snapshot) => {
                self.explorer.repository_changed(snapshot);
                self.history.repository_changed(snapshot);
            }
            Message::OperationCompleted(_, _, snapshot) => {
                self.explorer.repository_changed(snapshot);
                self.history.repository_changed(snapshot);
            }
            _ => {}
        }
        let effect = update(&mut self.diff.model, message);
        if review_mode_changed {
            let mode = self.diff.model.review.diff_view_mode;
            self.selected_review_mode = Some(mode);
            self.history.set_review_mode(mode);
        }
        if !preparation_owned && self.diff.model != model_before {
            self.request_redraw();
        }
        match effect {
            Some(Effect::Repository(action)) => {
                if commit_submission {
                    self.close_modal();
                }
                self.commands.enqueue(action);
                None
            }
            Some(Effect::Toast(kind, title)) => {
                self.show_toast(kind, title);
                None
            }
            Some(Effect::Error(title, detail)) => {
                self.show_error(title, detail);
                None
            }
            None => None,
        }
    }

    fn active_commands(&self) -> &'static [Command] {
        match self.active {
            Activity::Diff => self.diff.commands(),
            Activity::Explorer => self.explorer.commands(),
            Activity::History => self.history.commands(),
        }
    }

    fn open_active_palette(&mut self) {
        let mut commands = SHARED_COMMANDS.to_vec();
        if let Some(command) = merge::palette_command(&self.diff.model.snapshot) {
            commands.insert(3, command);
        }
        commands.extend_from_slice(self.active_commands());
        self.set_modal(Modal::command_palette(commands));
    }

    fn execute_palette_command(&mut self, command: CommandId) -> Option<WorkbenchEffect> {
        let action = if command == FETCH_COMMAND {
            Some(RepositoryAction::Fetch)
        } else if command == SYNC_COMMAND {
            let _ = self.execute_sync();
            return None;
        } else if self.execute_merge_command(command)
            || self.execute_branch_picker_command(command)
            || self.execute_create_branch_command(command)
        {
            return None;
        } else if command == UPDATE_COMMAND {
            self.commands.enqueue_update();
            return None;
        } else {
            None
        };
        if let Some(action) = action {
            self.commands.enqueue(action);
            return None;
        }
        match self.active {
            Activity::Diff => self.diff.execute_command(command),
            Activity::Explorer => self.explorer.execute_command(command),
            Activity::History => self.history.execute_command(command),
        };
        None
    }

    pub fn show_toast(&mut self, kind: ToastKind, message: impl Into<String>) {
        self.toasts.show(kind, message);
        self.request_redraw();
    }

    pub fn show_error(&mut self, title: impl Into<String>, detail: impl Into<String>) {
        let error = ErrorDialog::new(title, detail);
        if matches!(self.modal, Some(Modal::GitPrompt(_) | Modal::Error(_))) {
            if !self.pending_errors.contains(&error)
                && !matches!(self.modal, Some(Modal::Error(ref visible)) if visible == &error)
            {
                self.pending_errors.push_back(error);
            }
            return;
        }
        self.set_modal(Modal::Error(error));
    }
}

mod tool_impls;
use tool_impls::{explorer_frame_preparation, explorer_preparation};

#[cfg(test)]
mod repository_update_tests;
#[cfg(test)]
mod tests;
