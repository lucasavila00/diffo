#![doc = include_str!("../README.md")]

use std::{collections::HashMap, time::Instant};

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use diffo_app::{Effect, Message, Model, ToastKind, ToastQueue, update};
use diffo_command::{Command, CommandId, CommandPalette, PaletteEvent};
use diffo_core::{OperationFailure, OperationResult, RepositoryAction, RepositorySnapshot};
use diffo_text_view::{TextRenderMode, TextSurfacePreparation};
use diffo_tui::{FramePreparation, Renderer, RendererEvent, render_toasts, toast_at_position};
use diffo_ui::{PaneSplit, tool_areas};
use ratatui::{Frame, layout::Rect, widgets::Clear};

use diffo_explorer::{ExplorerActivity, ExplorerEvent, ExplorerOutcome, ExplorerRequest};

mod activity_bar;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkbenchEffect {
    Repository(RepositoryAction),
    CopyPath {
        path: std::path::PathBuf,
        absolute: bool,
    },
}

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
    should_quit: bool,
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
            should_quit: false,
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

    pub fn tick(&mut self) {
        self.expire_toasts();
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
        render_toasts(frame, self.toasts.as_slice(), content);
        self.active_palette().render(frame, content);
        render_activity_bar(frame, area, self.active);
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
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && key.code == KeyCode::Tab
            && key.modifiers == KeyModifiers::NONE
        {
            self.active = self.active.next();
            return None;
        }
        if let Event::Mouse(mouse) = event
            && mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && let Some(activity) = activity_at_position(area, mouse.column, mouse.row)
        {
            self.active = activity;
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
        if !tool_captures_global_input && self.dismiss_clicked_toast(event, content) {
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

    fn sync_diff_pane_state(&mut self) {
        self.diff.model.file_pane_percent = self.pane_split.percent();
        self.diff.model.resizing_file_pane = self.pane_split.is_dragging();
    }

    pub fn take_task(&mut self) -> Option<WorkbenchTask> {
        self.explorer.take_request().map(WorkbenchTask::Explorer)
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
            Some(Effect::Repository(action)) => Some(WorkbenchEffect::Repository(action)),
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
            return self
                .diff
                .model
                .start_repository_action(action)
                .map(WorkbenchEffect::Repository);
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
        action: RepositoryAction,
        result: OperationResult,
        snapshot: RepositorySnapshot,
    ) {
        let _ = self.update_diff(Message::OperationCompleted(action, result, snapshot));
    }

    pub fn action_failed(&mut self, failure: OperationFailure) {
        let _ = self.update_diff(Message::ActionFailed(failure));
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

#[derive(Default)]
struct PendingScroll {
    vertical: i64,
    horizontal: i64,
}

impl PendingScroll {
    fn push(&mut self, message: &Message) -> bool {
        match message {
            Message::ScrollDiffUp => self.vertical = self.vertical.saturating_sub(4),
            Message::ScrollDiffDown => self.vertical = self.vertical.saturating_add(4),
            Message::ScrollDiffPageUp(lines) => {
                self.vertical = self
                    .vertical
                    .saturating_sub(i64::try_from(*lines).unwrap_or(i64::MAX));
            }
            Message::ScrollDiffPageDown(lines) => {
                self.vertical = self
                    .vertical
                    .saturating_add(i64::try_from(*lines).unwrap_or(i64::MAX));
            }
            Message::ScrollDiffVerticalBy(lines) => {
                self.vertical = self.vertical.saturating_add(*lines);
            }
            Message::ScrollDiffLeft => self.horizontal = self.horizontal.saturating_sub(4),
            Message::ScrollDiffRight => self.horizontal = self.horizontal.saturating_add(4),
            Message::ScrollDiffHorizontalBy(columns) => {
                self.horizontal = self.horizontal.saturating_add(*columns);
            }
            _ => return false,
        }
        true
    }

    fn flush(&mut self, workbench: &mut Workbench) {
        if self.vertical != 0 {
            let _ = workbench.update_diff(Message::ScrollDiffVerticalBy(self.vertical));
        }
        if self.horizontal != 0 {
            let _ = workbench.update_diff(Message::ScrollDiffHorizontalBy(self.horizontal));
        }
        *self = Self::default();
    }
}

impl Tool for DiffActivity {
    fn handle_event(
        &mut self,
        event: &Event,
        area: Rect,
        _split: PaneSplit,
    ) -> Option<WorkbenchCommand> {
        self.renderer
            .map_event(event, &self.model, area)
            .and_then(|event| match event {
                RendererEvent::Consumed => None,
                RendererEvent::Message(message) => Some(WorkbenchCommand::Diff(message)),
                RendererEvent::CopyPath { path, absolute } => {
                    Some(WorkbenchCommand::Effect(WorkbenchEffect::CopyPath {
                        path,
                        absolute,
                    }))
                }
            })
    }

    fn prepare_frame(&mut self, area: Rect, _split: PaneSplit) -> FramePreparation {
        let preparation = self.renderer.prepare_frame(&self.model, area);
        if let Some(viewport) = preparation.viewport_transition {
            self.model
                .set_diff_viewport(viewport.vertical, viewport.horizontal);
        }
        self.model.clamp_diff_scroll(
            preparation.maximum_vertical_scroll,
            preparation.maximum_horizontal_scroll,
        );
        preparation
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _split: PaneSplit) {
        self.renderer.render_in(frame, &self.model, area);
    }

    fn is_preparing(&self) -> bool {
        self.renderer.is_preparing()
    }

    fn captures_global_input(&self) -> bool {
        self.model.commit_input_focused()
            || self.model.help_open
            || self.renderer.has_open_picker_menu()
    }
}

impl Tool for ExplorerActivity {
    fn handle_event(
        &mut self,
        event: &Event,
        area: Rect,
        split: PaneSplit,
    ) -> Option<WorkbenchCommand> {
        ExplorerActivity::handle_event(self, event, area, split).and_then(|event| match event {
            ExplorerEvent::Consumed => None,
            ExplorerEvent::CopyPath { path, absolute } => {
                Some(WorkbenchCommand::Effect(WorkbenchEffect::CopyPath {
                    path,
                    absolute,
                }))
            }
        })
    }

    fn prepare_frame(&mut self, area: Rect, split: PaneSplit) -> FramePreparation {
        explorer_preparation(ExplorerActivity::prepare_frame(self, area, split))
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, split: PaneSplit) {
        ExplorerActivity::render(self, frame, area, split);
    }

    fn is_preparing(&self) -> bool {
        ExplorerActivity::is_preparing(self)
    }

    fn captures_global_input(&self) -> bool {
        self.has_open_picker_menu()
    }

    fn commands(&self) -> &'static [Command] {
        ExplorerActivity::commands(self)
    }

    fn execute_command(&mut self, command: CommandId) -> bool {
        ExplorerActivity::execute_command(self, command)
    }
}

fn explorer_preparation(text_surface: TextSurfacePreparation) -> FramePreparation {
    FramePreparation {
        content_revision: text_surface.document_revision,
        preparing: text_surface.mode == TextRenderMode::TextSkeleton,
        syntax_ready: text_surface.mode == TextRenderMode::Full,
        text_surface: Some(text_surface),
        ..FramePreparation::default()
    }
}

impl Tool for SearchActivity {
    fn handle_event(
        &mut self,
        _event: &Event,
        _area: Rect,
        _split: PaneSplit,
    ) -> Option<WorkbenchCommand> {
        None
    }

    fn prepare_frame(&mut self, _area: Rect, _split: PaneSplit) -> FramePreparation {
        FramePreparation::default()
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, split: PaneSplit) {
        frame.render_widget(Clear, area);
        let content = tool_areas(area).content;
        let panes = split.areas(content);
        frame.render_widget(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(split.border_style()),
            panes.leading,
        );
        frame.render_widget(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_style(split.border_style()),
            panes.trailing,
        );
    }

    fn is_preparing(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventState, MouseEvent};
    use diffo_app::NetworkOperation;
    use diffo_explorer::COLLAPSE_ALL_COMMAND;
    use ratatui::{Terminal, backend::TestBackend};

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn tab_cycles_activities_without_changing_diff_state() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.diff.model.diff_scroll = 17;
        let tab = Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let area = Rect::new(0, 0, 100, 30);

        let _ = workbench.handle_event(&tab, area);
        assert_eq!(workbench.active, Activity::Explorer);
        let _ = workbench.handle_event(&tab, area);
        assert_eq!(workbench.active, Activity::Search);
        let _ = workbench.handle_event(&tab, area);
        assert_eq!(workbench.active, Activity::Diff);
        assert_eq!(workbench.diff.model.diff_scroll, 17);
    }

    #[test]
    fn activity_bar_click_selects_and_consumes_the_activity() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 4,
            modifiers: KeyModifiers::NONE,
        });

        let _ = workbench.handle_event(&click, Rect::new(0, 0, 100, 30));
        assert_eq!(workbench.active, Activity::Search);
    }

    #[test]
    fn pane_drag_is_shared_across_activities() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let area = Rect::new(0, 0, 100, 30);
        let pane_area = tool_areas(workbench_areas(area).content).content;
        let seam = workbench.pane_split.areas(pane_area).trailing.x;
        let mouse = |kind, column| {
            Event::Mouse(MouseEvent {
                kind,
                column,
                row: pane_area.y.saturating_add(2),
                modifiers: KeyModifiers::NONE,
            })
        };

        let _ = workbench.handle_event(&mouse(MouseEventKind::Down(MouseButton::Left), seam), area);
        let _ = workbench.handle_event(&mouse(MouseEventKind::Drag(MouseButton::Left), 62), area);
        let _ = workbench.handle_event(&mouse(MouseEventKind::Up(MouseButton::Left), 62), area);

        assert_eq!(workbench.pane_split.percent(), 60);
        assert_eq!(workbench.diff.model.file_pane_percent, 60);
        let _ = workbench.handle_event(&key(KeyCode::Tab), area);
        assert_eq!(workbench.active, Activity::Explorer);
        assert_eq!(workbench.pane_split.areas(pane_area).trailing.x, 62);
        let _ = workbench.handle_event(&key(KeyCode::Tab), area);
        assert_eq!(workbench.active, Activity::Search);
        assert_eq!(workbench.pane_split.areas(pane_area).trailing.x, 62);
    }

    #[test]
    fn pane_toggle_is_global_and_diff_overlays_capture_input() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let area = Rect::new(0, 0, 100, 30);

        let _ = workbench.handle_event(&key(KeyCode::Tab), area);
        let _ = workbench.handle_event(&key(KeyCode::Char('e')), area);
        assert_eq!(workbench.pane_split.percent(), 0);
        let _ = workbench.handle_event(&key(KeyCode::Char('e')), area);
        assert_eq!(workbench.pane_split.percent(), 25);

        workbench.active = Activity::Diff;
        workbench.diff.model.help_open = true;
        let _ = workbench.handle_event(&key(KeyCode::Char('e')), area);
        assert_eq!(workbench.pane_split.percent(), 25);
    }

    #[test]
    fn explorer_picker_menu_captures_global_shortcuts() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let area = Rect::new(0, 0, 100, 30);
        workbench.active = Activity::Explorer;
        workbench.explorer.accept(ExplorerOutcome::Paths {
            id: 1,
            result: Ok(vec![std::path::PathBuf::from("file.txt")]),
        });
        workbench.prepare_frame(area);
        let pane_area = tool_areas(workbench_areas(area).content).content;
        let tree = workbench.pane_split.areas(pane_area).leading;
        let right_click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: tree.x.saturating_add(2),
            row: tree.y.saturating_add(1),
            modifiers: KeyModifiers::NONE,
        });

        let _ = workbench.handle_event(&right_click, area);
        assert!(workbench.explorer.has_open_picker_menu());

        for code in [KeyCode::Char('e'), KeyCode::Char('1'), KeyCode::Char('q')] {
            let _ = workbench.handle_event(&key(code), area);
        }
        assert_eq!(workbench.pane_split.percent(), 25);
        assert!(!workbench.active_palette().is_open());
        assert!(!workbench.should_quit());
        assert!(workbench.explorer.has_open_picker_menu());

        let _ = workbench.handle_event(&key(KeyCode::Esc), area);
        assert!(!workbench.explorer.has_open_picker_menu());
        assert!(!workbench.should_quit());
    }

    #[test]
    fn tab_requires_an_unmodified_key_press() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let repeat = Event::Key(crossterm::event::KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Repeat,
            state: KeyEventState::NONE,
        });
        let modified = Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT));

        let _ = workbench.handle_event(&repeat, Rect::default());
        let _ = workbench.handle_event(&modified, Rect::default());
        assert_eq!(workbench.active, Activity::Diff);
    }

    #[test]
    fn empty_activities_keep_quit_available() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.active = Activity::Explorer;
        let quit = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));

        let _ = workbench.handle_event(&quit, Rect::default());
        assert!(workbench.should_quit());
    }

    #[test]
    fn empty_search_draws_the_shared_page_panes() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.active = Activity::Search;
        let backend = TestBackend::new(20, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| workbench.render(frame)).unwrap();

        let pane_area = tool_areas(workbench_areas(Rect::new(0, 0, 20, 12)).content).content;
        let seam = workbench.pane_split.areas(pane_area).trailing.x;
        assert_eq!(
            terminal.backend().buffer()[(seam, pane_area.y)].symbol(),
            "┌"
        );
    }

    #[test]
    fn palettes_keep_separate_state_for_each_activity() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let area = Rect::new(0, 0, 100, 30);

        let _ = workbench.handle_event(&key(KeyCode::Char('1')), area);
        let _ = workbench.handle_event(&key(KeyCode::Char('p')), area);
        let _ = workbench.handle_event(&key(KeyCode::Tab), area);
        let _ = workbench.handle_event(&key(KeyCode::Char('1')), area);
        let _ = workbench.handle_event(&key(KeyCode::Char('c')), area);
        let _ = workbench.handle_event(&key(KeyCode::Tab), area);
        let _ = workbench.handle_event(&key(KeyCode::Tab), area);

        assert_eq!(workbench.active, Activity::Diff);
        assert_eq!(workbench.active_palette().query(), "p");
        let _ = workbench.handle_event(&key(KeyCode::Tab), area);
        assert_eq!(workbench.active_palette().query(), "c");
        assert!(
            workbench
                .active_palette()
                .matches()
                .iter()
                .any(|command| command.id == COLLAPSE_ALL_COMMAND)
        );
    }

    #[test]
    fn shared_git_commands_execute_from_every_activity() {
        let area = Rect::new(0, 0, 100, 30);
        for activity in [Activity::Diff, Activity::Explorer, Activity::Search] {
            let mut workbench = Workbench::new(RepositorySnapshot::default());
            workbench.active = activity;

            let effects =
                workbench.handle_events(&[key(KeyCode::Char('1')), key(KeyCode::Enter)], area);

            assert_eq!(
                effects,
                vec![WorkbenchEffect::Repository(RepositoryAction::Fetch)]
            );
            assert_eq!(
                workbench.diff.model.network_operation(),
                Some(NetworkOperation::Fetch)
            );
        }
    }

    #[test]
    fn operation_toasts_render_in_diff_and_explorer() {
        for activity in [Activity::Diff, Activity::Explorer] {
            let mut workbench = Workbench::new(RepositorySnapshot::default());
            workbench.active = activity;
            assert_eq!(
                workbench
                    .diff
                    .model
                    .start_repository_action(RepositoryAction::Pull),
                Some(RepositoryAction::Pull)
            );
            workbench.operation_completed(
                RepositoryAction::Pull,
                OperationResult::Pull { commits: 1 },
                RepositorySnapshot::default(),
            );
            assert_eq!(workbench.diff.model.network_operation(), None);
            assert_eq!(workbench.toasts.as_slice()[0].title, "Pulled 1 commit");
            let backend = TestBackend::new(100, 30);
            let mut terminal = Terminal::new(backend).unwrap();

            terminal.draw(|frame| workbench.render(frame)).unwrap();

            let screen = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(ratatui::buffer::Cell::symbol)
                .collect::<String>();
            assert!(
                screen.contains("Pulled 1 commit"),
                "missing operation toast in {activity:?}"
            );
        }
    }

    #[test]
    fn explorer_can_click_dismiss_a_workbench_toast() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.active = Activity::Explorer;
        workbench.show_toast(ToastKind::Info, "Fetch complete");
        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 70,
            row: 26,
            modifiers: KeyModifiers::NONE,
        });

        let _ = workbench.handle_event(&click, Rect::new(0, 0, 100, 30));

        assert!(workbench.toasts.as_slice().is_empty());
    }

    #[test]
    fn network_activity_does_not_own_the_toast_queue() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.show_toast(ToastKind::Info, "Existing result");
        let existing_id = workbench.toasts.as_slice()[0].id;

        assert_eq!(
            workbench
                .diff
                .model
                .start_repository_action(RepositoryAction::Fetch),
            Some(RepositoryAction::Fetch)
        );
        assert_eq!(
            workbench.diff.model.network_operation(),
            Some(NetworkOperation::Fetch)
        );
        assert!(
            workbench
                .toasts
                .as_slice()
                .iter()
                .any(|toast| toast.id == existing_id)
        );

        workbench.operation_completed(
            RepositoryAction::Fetch,
            OperationResult::Fetch { updated_refs: 0 },
            RepositorySnapshot::default(),
        );

        assert_eq!(workbench.diff.model.network_operation(), None);
        assert!(
            workbench
                .toasts
                .as_slice()
                .iter()
                .any(|toast| toast.id == existing_id)
        );
    }

    #[test]
    fn command_palette_shortcut_does_not_capture_commit_message_input() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        let _ = update(&mut workbench.diff.model, Message::FocusCommitInput);

        let effects = workbench.handle_events(&[key(KeyCode::Char('1'))], Rect::default());

        assert!(effects.is_empty());
        assert_eq!(workbench.diff.model.commit_message, "1");
        assert!(!workbench.active_palette().is_open());
    }

    #[test]
    fn explorer_palette_combines_shared_and_explorer_commands() {
        let mut workbench = Workbench::new(RepositorySnapshot::default());
        workbench.active = Activity::Explorer;
        workbench.open_active_palette();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| workbench.render(frame)).unwrap();

        let screen = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(screen.contains("Git: Fetch"));
        assert!(screen.contains("Explorer: Collapse All Folders"));
    }
}
