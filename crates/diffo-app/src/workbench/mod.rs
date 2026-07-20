//! Activity composition, global input routing, and command lifecycle.

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use crate::diff::{
    CommandProgress, FramePreparation, Renderer, RendererEvent, command_cancel_at_position,
    render_command_progress, render_status, render_toasts, toast_at_position,
};
use crate::diff::{Effect, Message, Model, ToastKind, ToastQueue, update};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_core::{
    ApplicationCommandId, GitPrompt, OperationFailure, PromptId, RepositoryAction,
    RepositoryQueryId, RepositorySnapshot,
};
use diffo_ui::command_palette::{Command, CommandId};
use diffo_ui::text_view::{TextRenderMode, TextSurfacePreparation};
use diffo_ui::{PaneSplit, command_progress_style, icons, mouse_target_style, tool_areas};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::explorer::{ExplorerActivity, ExplorerEvent, ExplorerOutcome, ExplorerRequest};

mod activity_bar;
mod application_update;
mod bindings;
mod checkout_picker;
mod command_queue;
mod full_screen;
mod help;
mod modal;
mod pending_scroll;
mod prompt;
mod repository_update;

use bindings::GlobalAction;
use modal::Modal;
use pending_scroll::PendingScroll;
#[cfg(test)]
use prompt::{ConfirmChoice, prompt_layout};
use prompt::{PromptModal, render_prompt};

pub use command_queue::{
    ApplicationAction, ApplicationCommand, CommandQueue, CommandResult, CommandState,
};

pub use activity_bar::{
    ACTIVITY_BAR_WIDTH, WorkbenchAreas, activity_at_position, render_activity_bar, workbench_areas,
};
pub use application_update::UpdateOutcome;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Activity {
    #[default]
    Diff,
    Explorer,
    Search,
}

enum WorkbenchCommand {
    Diff(Message),
    Effect(WorkbenchEffect),
}

#[derive(Debug, Eq, PartialEq)]
pub enum WorkbenchEffect {
    CopyPath {
        path: std::path::PathBuf,
        absolute: bool,
    },
    Prompt {
        command_id: ApplicationCommandId,
        prompt_id: PromptId,
        response: PromptResponse,
    },
}

pub enum PromptResponse {
    Text(String),
    Confirm,
    Cancel,
}

impl std::fmt::Debug for PromptResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(_) => formatter.write_str("Text([redacted])"),
            Self::Confirm => formatter.write_str("Confirm"),
            Self::Cancel => formatter.write_str("Cancel"),
        }
    }
}

impl PartialEq for PromptResponse {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Text(left), Self::Text(right)) => left == right,
            (Self::Confirm, Self::Confirm) | (Self::Cancel, Self::Cancel) => true,
            _ => false,
        }
    }
}

impl Eq for PromptResponse {}

pub enum WorkbenchTask {
    Explorer(ExplorerRequest),
}

pub enum WorkbenchTaskResult {
    Explorer(ExplorerOutcome),
}

pub struct Workbench {
    active: Activity,
    diff: DiffActivity,
    explorer: ExplorerActivity,
    search: SearchActivity,
    pane_split: PaneSplit,
    toasts: ToastQueue,
    toast_deadlines: HashMap<u64, Instant>,
    persistent_toasts: HashSet<u64>,
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
    next_query_id: u64,
}

struct DiffActivity {
    model: Model,
    renderer: Renderer,
}

struct SearchActivity;

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

const SHARED_COMMANDS: [Command; 4] = [
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
        Self {
            active: Activity::Diff,
            diff: DiffActivity {
                model: Model::new(snapshot),
                renderer: Renderer::new(),
            },
            explorer,
            search: SearchActivity,
            pane_split: PaneSplit::default(),
            toasts: ToastQueue::new(),
            toast_deadlines: HashMap::new(),
            persistent_toasts: HashSet::new(),
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
            next_query_id: 1,
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

    pub fn tick(&mut self, now: Instant) {
        self.expire_toasts(now);
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
        } else {
            self.command_animation_tick = 0;
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

    #[must_use]
    pub const fn full_screen(&self) -> bool {
        self.full_screen
    }

    pub fn prepare_frame(&mut self, area: Rect) -> FramePreparation {
        if let Some(preparation) = self.prepare_full_screen(area) {
            return preparation;
        }
        let content = workbench_areas(area).content;
        self.sync_diff_pane_state();
        match self.active {
            Activity::Diff => self.diff.prepare_frame(content, self.pane_split),
            Activity::Explorer => {
                explorer_preparation(self.explorer.prepare_frame(content, self.pane_split))
            }
            Activity::Search => self.search.prepare_frame(content, self.pane_split),
        }
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        if self.render_full_screen(frame) {
            return;
        }
        let content = workbench_areas(area).content;
        match self.active {
            Activity::Diff => self.diff.render(frame, content, self.pane_split),
            Activity::Explorer => self.explorer.render(frame, content, self.pane_split),
            Activity::Search => self.search.render(frame, content, self.pane_split),
        }
        render_status(frame, tool_areas(content).status, &self.diff.model);
        self.render_full_screen_entry(frame);
        render_pane_drag_marker(frame, tool_areas(content).content, self.pane_split);
        render_toasts(frame, self.toasts.as_slice(), content);
        if let Some(command) = self
            .commands
            .active()
            .filter(|_| self.command_progress.is_visible())
        {
            render_command_progress(
                frame,
                CommandProgress {
                    label: &command.label,
                    cancelling: command.state == CommandState::Cancelling,
                    animation_tick: self.command_animation_tick,
                },
                content,
            );
        }
        render_activity_bar(frame, area, self.active);
        if self.command_progress.is_visible() {
            frame.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(command_progress_style(self.command_animation_tick)),
                area,
            );
        }
        self.render_modal(frame, content, area);
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
            }
        }
        scroll.flush(self);
        effects
    }

    fn handle_event(&mut self, event: &Event, area: Rect) -> Option<WorkbenchCommand> {
        if self.modal.is_some() {
            return self.handle_modal_event(event, area);
        }
        if !self.full_screen && self.select_activity(event, area) {
            return None;
        }
        let content = workbench_areas(area).content;
        let tool_captures_global_input = match self.active {
            Activity::Diff => self.diff.captures_global_input(),
            Activity::Explorer => self.explorer.captures_global_input(),
            Activity::Search => self.search.captures_global_input(),
        };
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
            return None;
        }
        if !tool_captures_global_input && self.request_full_screen(event, area) {
            return None;
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
            return None;
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
                    return None;
                }
                MouseEventKind::Drag(MouseButton::Left) if self.pane_split.is_dragging() => {
                    self.pane_split.drag_to(pane_area, mouse.column);
                    self.sync_diff_pane_state();
                    return None;
                }
                MouseEventKind::Up(MouseButton::Left) if self.pane_split.is_dragging() => {
                    self.pane_split.end_drag();
                    self.sync_diff_pane_state();
                    return None;
                }
                _ => {}
            }
        }
        if !tool_captures_global_input && let Some(action) = bindings::action(event) {
            match action {
                GlobalAction::OpenCommandPalette => self.open_active_palette(),
                GlobalAction::ToggleHelp => self.set_modal(Modal::Help),
                GlobalAction::Sync => return self.execute_sync(),
            }
            return None;
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
        match self.active {
            Activity::Diff => self.diff.handle_event(event, content, self.pane_split),
            Activity::Explorer => {
                Tool::handle_event(&mut self.explorer, event, content, self.pane_split)
            }
            Activity::Search => self.search.handle_event(event, content, self.pane_split),
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
            return true;
        }
        if let Event::Mouse(mouse) = event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && let Some(activity) = activity_at_position(area, mouse.column, mouse.row)
        {
            self.dismiss_active_popover();
            self.active = activity;
            return true;
        }
        false
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
        self.dismiss_clicked_toast(event, area) || self.cancel_clicked_command(event, area)
    }

    fn cancel_clicked_command(&mut self, event: &Event, area: Rect) -> bool {
        let Event::Mouse(mouse) = event else {
            return false;
        };
        if mouse.kind != MouseEventKind::Down(MouseButton::Left)
            || !command_cancel_at_position(area, mouse.column, mouse.row)
        {
            return false;
        }
        let Some(id) = self
            .commands
            .active()
            .filter(|_| self.command_progress.is_visible())
            .map(|command| command.id)
        else {
            return false;
        };
        self.commands.cancel(id)
    }

    fn sync_diff_pane_state(&mut self) {
        self.diff.model.file_pane_percent = self.pane_split.percent();
        self.diff.model.resizing_file_pane = self.pane_split.is_dragging();
    }

    fn dismiss_active_popover(&mut self) {
        match self.active {
            Activity::Diff => self.diff.dismiss_popover(),
            Activity::Explorer => self.explorer.dismiss_popover(),
            Activity::Search => self.search.dismiss_popover(),
        }
    }

    pub fn take_application_command(&mut self, now: Instant) -> Option<ApplicationCommand> {
        let command = self.commands.start_next()?;
        self.last_prompt_id = None;
        self.command_progress = CommandProgressState::Waiting {
            command_id: command.id,
            reveal_at: now + Duration::from_millis(150),
        };
        self.command_animation_tick = 0;
        if let ApplicationAction::Repository(action) = &command.action {
            let _ = self.diff.model.start_repository_action(action.clone());
        }
        Some(command)
    }

    pub fn accept_task_result(&mut self, result: WorkbenchTaskResult) {
        match result {
            WorkbenchTaskResult::Explorer(outcome) => self.explorer.accept(outcome),
        }
    }

    fn update_diff(&mut self, message: Message) -> Option<WorkbenchEffect> {
        if message == Message::FocusCommitInput {
            self.set_modal(Modal::CommitEditor);
            return None;
        }
        if message == Message::BlurCommitInput {
            self.close_modal();
            return None;
        }
        let commit_submission = message == Message::ExecuteCommit;
        let reopen_commit_editor = matches!(
            &message,
            Message::ActionFailed(OperationFailure {
                action: RepositoryAction::Commit(_),
                ..
            })
        );
        match &message {
            Message::SnapshotLoaded(snapshot) | Message::OperationCompleted(_, _, snapshot) => {
                self.explorer.repository_changed(snapshot);
            }
            _ => {}
        }
        let effect = match update(&mut self.diff.model, message) {
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
            None => None,
        };
        if reopen_commit_editor {
            self.set_modal(Modal::CommitEditor);
        }
        effect
    }

    fn active_commands(&self) -> &'static [Command] {
        match self.active {
            Activity::Diff => self.diff.commands(),
            Activity::Explorer => self.explorer.commands(),
            Activity::Search => self.search.commands(),
        }
    }

    fn open_active_palette(&mut self) {
        let commands = SHARED_COMMANDS
            .iter()
            .chain(self.active_commands())
            .copied()
            .collect::<Vec<_>>();
        self.set_modal(Modal::command_palette(commands));
    }

    fn execute_palette_command(&mut self, command: CommandId) -> Option<WorkbenchEffect> {
        let action = if command == FETCH_COMMAND {
            Some(RepositoryAction::Fetch)
        } else if command == SYNC_COMMAND {
            return self.update_diff(Message::ExecuteSync);
        } else if command == CHECKOUT_COMMAND {
            self.open_checkout_picker();
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
            Activity::Search => self.search.execute_command(command),
        };
        None
    }

    pub fn show_toast(&mut self, kind: ToastKind, message: impl Into<String>) {
        self.toasts.show(kind, message);
    }
}

fn render_pane_drag_marker(frame: &mut Frame, area: Rect, split: PaneSplit) {
    let marker = split.seam_marker_area(area);
    if !marker.is_empty() {
        frame.render_widget(
            Paragraph::new(icons::PANE_DRAG).style(mouse_target_style()),
            marker,
        );
    }
}

mod tool_impls;
use tool_impls::explorer_preparation;

#[cfg(test)]
mod repository_update_tests;
#[cfg(test)]
mod tests;
