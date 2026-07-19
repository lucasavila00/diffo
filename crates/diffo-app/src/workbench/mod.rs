//! Activity composition, global input routing, and command lifecycle.

use std::{collections::HashMap, time::Instant};

use crate::diff::{
    CommandProgress, FramePreparation, Renderer, RendererEvent, command_cancel_at_position,
    render_command_progress, render_toasts, toast_at_position,
};
use crate::diff::{Effect, Message, Model, ToastKind, ToastQueue, update};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_core::{
    ApplicationCommandId, GitPrompt, OperationFailure, OperationResult, PromptId, RepositoryAction,
    RepositorySnapshot,
};
use diffo_ui::command_palette::{Command, CommandId, CommandPalette, PaletteEvent};
use diffo_ui::text_view::{TextRenderMode, TextSurfacePreparation};
use diffo_ui::{PaneSplit, command_progress_style, enabled_control_style, interaction, tool_areas};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::explorer::{ExplorerActivity, ExplorerEvent, ExplorerOutcome, ExplorerRequest};

mod activity_bar;
mod command_queue;
mod pending_scroll;
mod prompt;

use pending_scroll::PendingScroll;
#[cfg(test)]
use prompt::{ConfirmChoice, prompt_layout};
use prompt::{PromptModal, render_prompt};

pub use command_queue::{ApplicationCommand, CommandQueue, CommandResult, CommandState};

pub use activity_bar::{
    ACTIVITY_BAR_WIDTH, WorkbenchAreas, activity_at_position, render_activity_bar, workbench_areas,
};

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

impl Activity {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Diff => Self::Explorer,
            Self::Explorer => Self::Search,
            Self::Search => Self::Diff,
        }
    }
}

pub struct Workbench {
    active: Activity,
    diff: DiffActivity,
    explorer: ExplorerActivity,
    search: SearchActivity,
    palettes: ActivityPalettes,
    pane_split: PaneSplit,
    toasts: ToastQueue,
    toast_deadlines: HashMap<u64, Instant>,
    commands: CommandQueue,
    command_animation_tick: usize,
    should_quit: bool,
    prompt: Option<PromptModal>,
    last_prompt_id: Option<PromptId>,
}

struct DiffActivity {
    model: Model,
    renderer: Renderer,
}

struct SearchActivity;

#[derive(Default)]
struct ActivityPalettes {
    diff: CommandPalette,
    explorer: CommandPalette,
    search: CommandPalette,
}

const FETCH_COMMAND: CommandId = CommandId::new("git.fetch");
const PULL_COMMAND: CommandId = CommandId::new("git.pull");

const SHARED_COMMANDS: [Command; 2] = [
    Command {
        id: FETCH_COMMAND,
        label: "Git: Fetch",
    },
    Command {
        id: PULL_COMMAND,
        label: "Git: Pull",
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
    fn execute_command(&mut self, _command: CommandId) -> bool {
        false
    }
}

impl Workbench {
    #[must_use]
    pub fn new(snapshot: RepositorySnapshot) -> Self {
        Self {
            active: Activity::Diff,
            diff: DiffActivity {
                model: Model::new(snapshot.clone()),
                renderer: Renderer::new(),
            },
            explorer: ExplorerActivity::new(snapshot),
            search: SearchActivity,
            palettes: ActivityPalettes::default(),
            pane_split: PaneSplit::default(),
            toasts: ToastQueue::new(),
            toast_deadlines: HashMap::new(),
            commands: CommandQueue::new(),
            command_animation_tick: 0,
            should_quit: false,
            prompt: None,
            last_prompt_id: None,
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
        self.prompt
            .as_ref()
            .is_some_and(|modal| matches!(modal.prompt, GitPrompt::Secret { .. }))
    }

    pub fn tick(&mut self) {
        self.expire_toasts();
        if self.commands.active().is_some() {
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
    pub fn is_preparing(&self) -> bool {
        match self.active {
            Activity::Diff => self.diff.is_preparing(),
            Activity::Explorer => self.explorer.is_preparing(),
            Activity::Search => self.search.is_preparing(),
        }
    }

    pub fn prepare_frame(&mut self, area: Rect) -> FramePreparation {
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
        let content = workbench_areas(area).content;
        match self.active {
            Activity::Diff => self.diff.render(frame, content, self.pane_split),
            Activity::Explorer => self.explorer.render(frame, content, self.pane_split),
            Activity::Search => self.search.render(frame, content, self.pane_split),
        }
        render_pane_drag_marker(frame, tool_areas(content).content, self.pane_split);
        render_toasts(frame, self.toasts.as_slice(), content);
        if let Some(command) = self.commands.active() {
            render_command_progress(
                frame,
                CommandProgress {
                    label: command.label,
                    cancelling: command.state == CommandState::Cancelling,
                    animation_tick: self.command_animation_tick,
                },
                content,
            );
        }
        self.active_palette().render(frame, content);
        render_activity_bar(frame, area, self.active);
        if self.commands.active().is_some() {
            frame.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(command_progress_style(self.command_animation_tick)),
                area,
            );
        }
        if let Some(prompt) = self.prompt.as_ref() {
            render_prompt(frame, prompt, area);
        }
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
                WorkbenchCommand::Diff(Message::ToggleFilePane) => {
                    scroll.flush(self);
                    self.pane_split.toggle();
                    self.sync_diff_pane_state();
                }
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
        if self.prompt.is_some() {
            return self
                .handle_prompt_event(event, area)
                .map(WorkbenchCommand::Effect);
        }
        if self.select_activity(event, area) {
            return None;
        }
        let content = workbench_areas(area).content;
        if self.active_palette().is_open() {
            let palette_event = self.active_palette_mut().handle_event(event, content);
            return match palette_event {
                Some(PaletteEvent::Execute(command)) => self
                    .execute_palette_command(command)
                    .map(WorkbenchCommand::Effect),
                Some(PaletteEvent::Quit) => {
                    self.should_quit = true;
                    None
                }
                Some(PaletteEvent::Consumed) | None => None,
            };
        }
        let tool_captures_global_input = match self.active {
            Activity::Diff => self.diff.captures_global_input(),
            Activity::Explorer => self.explorer.captures_global_input(),
            Activity::Search => self.search.captures_global_input(),
        };
        if !tool_captures_global_input && self.handle_overlay_click(event, content) {
            return None;
        }
        if !tool_captures_global_input
            && let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && key.code == KeyCode::Char('e')
            && key.modifiers == KeyModifiers::NONE
        {
            self.pane_split.toggle();
            self.sync_diff_pane_state();
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
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && matches!(key.code, KeyCode::Char('1') | KeyCode::F(1))
            && key.modifiers == KeyModifiers::NONE
            && !tool_captures_global_input
        {
            self.open_active_palette();
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
            self.active = self.active.next();
            return true;
        }
        if let Event::Mouse(mouse) = event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && let Some(activity) = activity_at_position(area, mouse.column, mouse.row)
        {
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
        let Some(id) = self.commands.active().map(|command| command.id) else {
            return false;
        };
        self.commands.cancel(id)
    }

    fn sync_diff_pane_state(&mut self) {
        self.diff.model.file_pane_percent = self.pane_split.percent();
        self.diff.model.resizing_file_pane = self.pane_split.is_dragging();
    }

    pub fn take_task(&mut self) -> Option<WorkbenchTask> {
        self.explorer.take_request().map(WorkbenchTask::Explorer)
    }

    pub fn take_repository_command(&mut self) -> Option<ApplicationCommand> {
        let command = self.commands.start_next()?;
        self.prompt = None;
        self.last_prompt_id = None;
        let _ = self
            .diff
            .model
            .start_repository_action(command.action.clone());
        Some(command)
    }

    pub fn accept_task_result(&mut self, result: WorkbenchTaskResult) {
        match result {
            WorkbenchTaskResult::Explorer(outcome) => self.explorer.accept(outcome),
        }
    }

    fn update_diff(&mut self, message: Message) -> Option<WorkbenchEffect> {
        match &message {
            Message::SnapshotLoaded(snapshot) | Message::OperationCompleted(_, _, snapshot) => {
                self.explorer.repository_changed(snapshot.clone());
            }
            _ => {}
        }
        match update(&mut self.diff.model, message) {
            Some(Effect::Repository(action)) => {
                self.commands.enqueue(action);
                None
            }
            Some(Effect::Toast(kind, title)) => {
                self.show_toast(kind, title);
                None
            }
            None => None,
        }
    }

    fn active_palette(&self) -> &CommandPalette {
        match self.active {
            Activity::Diff => &self.palettes.diff,
            Activity::Explorer => &self.palettes.explorer,
            Activity::Search => &self.palettes.search,
        }
    }

    fn active_palette_mut(&mut self) -> &mut CommandPalette {
        match self.active {
            Activity::Diff => &mut self.palettes.diff,
            Activity::Explorer => &mut self.palettes.explorer,
            Activity::Search => &mut self.palettes.search,
        }
    }

    fn active_commands(&self) -> &'static [Command] {
        match self.active {
            Activity::Diff => self.diff.commands(),
            Activity::Explorer => self.explorer.commands(),
            Activity::Search => self.search.commands(),
        }
    }

    fn open_active_palette(&mut self) {
        if self.active == Activity::Diff {
            let _ = update(&mut self.diff.model, Message::CloseHelp);
        }
        let commands = SHARED_COMMANDS
            .iter()
            .chain(self.active_commands())
            .copied()
            .collect::<Vec<_>>();
        self.active_palette_mut().open(commands);
    }

    fn execute_palette_command(&mut self, command: CommandId) -> Option<WorkbenchEffect> {
        let action = if command == FETCH_COMMAND {
            Some(RepositoryAction::Fetch)
        } else if command == PULL_COMMAND {
            Some(RepositoryAction::Pull)
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

    pub fn repository_changed(&mut self, snapshot: RepositorySnapshot) {
        let _ = self.update_diff(Message::SnapshotLoaded(snapshot));
    }

    pub fn operation_failed(&mut self, message: String) {
        let _ = self.update_diff(Message::OperationFailed(message));
    }

    pub fn operation_completed(
        &mut self,
        id: ApplicationCommandId,
        action: RepositoryAction,
        result: OperationResult,
        snapshot: RepositorySnapshot,
    ) {
        if self
            .commands
            .acknowledge(id, CommandResult::Succeeded)
            .is_none()
        {
            return;
        }
        self.close_prompt(id);
        let _ = self.update_diff(Message::OperationCompleted(action, result, snapshot));
    }

    pub fn action_failed(&mut self, id: ApplicationCommandId, failure: OperationFailure) {
        if self
            .commands
            .acknowledge(id, CommandResult::Failed)
            .is_none()
        {
            return;
        }
        self.close_prompt(id);
        let _ = self.update_diff(Message::ActionFailed(failure));
    }

    pub fn operation_cancelled(&mut self, id: ApplicationCommandId, action: RepositoryAction) {
        if self
            .commands
            .acknowledge(id, CommandResult::Cancelled)
            .is_none()
        {
            return;
        }
        self.close_prompt(id);
        let _ = self.update_diff(Message::OperationCancelled(action));
    }

    fn expire_toasts(&mut self) {
        let now = Instant::now();
        self.toast_deadlines
            .retain(|id, _| self.toasts.as_slice().iter().any(|toast| toast.id == *id));
        for toast in self.toasts.as_slice() {
            if toast.kind != ToastKind::Error {
                self.toast_deadlines
                    .entry(toast.id)
                    .or_insert_with(|| now + std::time::Duration::from_secs(3));
            }
        }
        let expired = self
            .toast_deadlines
            .iter()
            .filter_map(|(id, deadline)| (*deadline <= now).then_some(*id))
            .collect::<Vec<_>>();
        for id in expired {
            self.toasts.dismiss(id);
            self.toast_deadlines.remove(&id);
        }
    }
}

fn render_pane_drag_marker(frame: &mut Frame, area: Rect, split: PaneSplit) {
    let marker = split.seam_marker_area(area);
    if !marker.is_empty() {
        frame.render_widget(
            Paragraph::new(interaction::PANE_DRAG).style(enabled_control_style()),
            marker,
        );
    }
}

mod tool_impls;
use tool_impls::explorer_preparation;

#[cfg(test)]
mod tests;
