use std::{
    collections::VecDeque,
    path::PathBuf,
    time::{Duration, Instant},
};

use crossterm::event::{Event, MouseButton, MouseEventKind};
use diffo_core::{ApplicationCommandId, CancellationHandle, RepositoryAction};
use ratatui::layout::Rect;

use crate::diff::{
    CommandProgress, CommandProgressRow, CommandProgressState as CommandRowState, FileKey, Message,
    command_at_position,
};

use super::{AiCommitRequest, CommandProgressState as WorkbenchProgressState, Workbench};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationAction {
    Repository(RepositoryAction),
    AiCommit(AiCommitRequest),
    Update,
}

pub(crate) enum CommandIntent {
    Repository(RepositoryAction),
    ToggleStage(FileKey),
    ToggleStageAll,
    StageAll,
    UnstageAll,
    StageFile(PathBuf),
    UnstageFile(PathBuf),
    Commit(String),
    AiCommit,
    Update,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandState {
    Queued,
    Running,
    Cancelling,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandResult {
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct ApplicationCommand {
    pub id: ApplicationCommandId,
    pub action: ApplicationAction,
    /// The complete user goal. Keep this stable while the command runs.
    pub label: String,
    /// The current step within the goal, when the command has multiple steps.
    pub phase: Option<String>,
    pub cancellation: CancellationHandle,
    pub state: CommandState,
}

pub(crate) struct QueuedCommand {
    pub id: ApplicationCommandId,
    pub intent: CommandIntent,
}

#[derive(Default)]
pub struct CommandQueue {
    queued: VecDeque<QueuedCommand>,
    active: Option<ApplicationCommand>,
    next_id: u64,
}

impl CommandQueue {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_id: 1,
            ..Self::default()
        }
    }

    pub fn enqueue(&mut self, action: RepositoryAction) -> ApplicationCommandId {
        self.enqueue_intent(CommandIntent::Repository(action))
    }

    pub fn enqueue_update(&mut self) -> ApplicationCommandId {
        self.enqueue_intent(CommandIntent::Update)
    }

    pub fn enqueue_ai_commit(&mut self) -> ApplicationCommandId {
        self.enqueue_intent(CommandIntent::AiCommit)
    }

    pub(crate) fn enqueue_intent(&mut self, intent: CommandIntent) -> ApplicationCommandId {
        let id = ApplicationCommandId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.queued.push_back(QueuedCommand { id, intent });
        id
    }

    #[must_use]
    pub fn active(&self) -> Option<&ApplicationCommand> {
        self.active.as_ref()
    }

    pub fn active_mut(&mut self) -> Option<&mut ApplicationCommand> {
        self.active.as_mut()
    }

    #[must_use]
    pub fn ai_commit_id(&self) -> Option<ApplicationCommandId> {
        self.active
            .iter()
            .find(|command| {
                matches!(
                    command.action,
                    ApplicationAction::AiCommit(_)
                        | ApplicationAction::Repository(RepositoryAction::GuardedCommit(_))
                )
            })
            .map(|command| command.id)
            .or_else(|| {
                self.queued
                    .iter()
                    .find(|command| matches!(command.intent, CommandIntent::AiCommit))
                    .map(|command| command.id)
            })
    }

    #[must_use]
    pub fn has_sync(&self) -> bool {
        self.active.as_ref().is_some_and(|command| {
            matches!(
                command.action,
                ApplicationAction::Repository(
                    RepositoryAction::Sync | RepositoryAction::SyncToRemote(_)
                )
            )
        }) || self.queued.iter().any(|command| {
            matches!(
                command.intent,
                CommandIntent::Repository(
                    RepositoryAction::Sync | RepositoryAction::SyncToRemote(_)
                )
            )
        })
    }

    #[must_use]
    pub fn queued_len(&self) -> usize {
        self.queued.len()
    }

    #[must_use]
    pub fn has_work(&self) -> bool {
        self.active.is_some() || !self.queued.is_empty()
    }

    pub fn entries(&self) -> impl Iterator<Item = (ApplicationCommandId, String, CommandState)> {
        self.active
            .iter()
            .map(|command| {
                let label = if command.state == CommandState::Running {
                    command.phase.as_ref().map_or_else(
                        || command.label.clone(),
                        |phase| format!("{} — {phase}", command.label),
                    )
                } else {
                    command.label.clone()
                };
                (command.id, label, command.state)
            })
            .chain(self.queued.iter().map(|command| {
                (
                    command.id,
                    intent_label(&command.intent),
                    CommandState::Queued,
                )
            }))
    }

    pub(crate) fn take_next(&mut self) -> Option<QueuedCommand> {
        if self.active.is_some() {
            return None;
        }
        self.queued.pop_front()
    }

    pub(crate) fn activate(
        &mut self,
        queued: QueuedCommand,
        action: ApplicationAction,
    ) -> ApplicationCommand {
        debug_assert!(self.active.is_none());
        let label = intent_label(&queued.intent);
        let command = ApplicationCommand {
            id: queued.id,
            label,
            phase: command_phase(&action),
            action,
            cancellation: CancellationHandle::default(),
            state: CommandState::Running,
        };
        self.active = Some(command.clone());
        command
    }

    pub fn cancel(&mut self, id: ApplicationCommandId) -> bool {
        if let Some(command) = self.active.as_mut().filter(|command| command.id == id) {
            command.cancellation.cancel();
            command.state = CommandState::Cancelling;
            self.queued.clear();
            return true;
        }
        let Some(index) = self.queued.iter().position(|command| command.id == id) else {
            return false;
        };
        self.queued.truncate(index);
        true
    }

    pub(crate) fn fail_preparation(&mut self) {
        self.queued.clear();
    }

    pub fn acknowledge(
        &mut self,
        id: ApplicationCommandId,
        result: CommandResult,
    ) -> bool {
        if !self.active.as_ref().is_some_and(|command| command.id == id) {
            return false;
        }
        let Some(command) = self.active.take() else {
            return false;
        };
        let explicitly_cancelled = command.state == CommandState::Cancelling;
        // cancel() already removed the old tail. Anything queued now was entered after
        // cancellation started and belongs to the next queue.
        if result != CommandResult::Succeeded
            && !(result == CommandResult::Cancelled && explicitly_cancelled)
        {
            self.queued.clear();
        }
        true
    }
}

fn intent_label(intent: &CommandIntent) -> String {
    match intent {
        CommandIntent::Repository(action) => {
            command_goal(&ApplicationAction::Repository(action.clone()))
        }
        CommandIntent::ToggleStage(_) => "Stage / unstage file".to_owned(),
        CommandIntent::ToggleStageAll => "Stage / unstage all".to_owned(),
        CommandIntent::StageAll => "Stage all".to_owned(),
        CommandIntent::UnstageAll => "Unstage all".to_owned(),
        CommandIntent::StageFile(_) => "Stage file".to_owned(),
        CommandIntent::UnstageFile(_) => "Unstage file".to_owned(),
        CommandIntent::Commit(_) => "Commit".to_owned(),
        CommandIntent::AiCommit => "AI commit".to_owned(),
        CommandIntent::Update => "Update Diffo".to_owned(),
    }
}

fn command_phase(action: &ApplicationAction) -> Option<String> {
    match action {
        ApplicationAction::Repository(
            RepositoryAction::Sync | RepositoryAction::SyncToRemote(_),
        ) => Some("Fetching".to_owned()),
        ApplicationAction::AiCommit(_) => Some("Generating commit message".to_owned()),
        _ => None,
    }
}

// This is the whole action the user asked for, never just its first phase. See ADR 0110.
fn command_goal(action: &ApplicationAction) -> String {
    match action {
        ApplicationAction::Repository(RepositoryAction::Stage(_) | RepositoryAction::StageAll) => {
            "Staging".to_owned()
        }
        ApplicationAction::Repository(
            RepositoryAction::Unstage(_) | RepositoryAction::UnstageAll,
        ) => "Unstaging".to_owned(),
        ApplicationAction::Repository(
            RepositoryAction::Sync | RepositoryAction::SyncToRemote(_),
        ) => "Sync".to_owned(),
        ApplicationAction::Repository(RepositoryAction::Fetch) => "Fetching".to_owned(),
        ApplicationAction::Repository(RepositoryAction::Commit(_)) => "Committing".to_owned(),
        ApplicationAction::Repository(RepositoryAction::GuardedCommit(_)) => {
            "Committing".to_owned()
        }
        ApplicationAction::Repository(RepositoryAction::Checkout(target)) => format!(
            "Checking out {}",
            target
                .full_ref
                .strip_prefix("refs/heads/")
                .or_else(|| target.full_ref.strip_prefix("refs/remotes/"))
                .unwrap_or(&target.full_ref)
        ),
        ApplicationAction::Repository(RepositoryAction::CreateBranch(target)) => {
            format!("Creating branch {}", target.name)
        }
        ApplicationAction::Repository(RepositoryAction::DeleteBranch(target)) => {
            format!("Deleting branch {}", target.name)
        }
        ApplicationAction::Repository(RepositoryAction::Merge(target)) => {
            format!("Merging {}", target.name)
        }
        ApplicationAction::Repository(RepositoryAction::AbortMerge) => "Aborting merge".to_owned(),
        ApplicationAction::Repository(RepositoryAction::Discard(_)) => {
            "Discarding changes".to_owned()
        }
        ApplicationAction::Repository(RepositoryAction::DiscardAll(_)) => {
            "Discarding all changes".to_owned()
        }
        ApplicationAction::Repository(RepositoryAction::Stash { .. }) => {
            "Stashing changes".to_owned()
        }
        ApplicationAction::Repository(RepositoryAction::ApplyStash(target)) => {
            format!("Applying {}", target.name)
        }
        ApplicationAction::Repository(RepositoryAction::DropStash(target)) => {
            format!("Dropping {}", target.name)
        }
        ApplicationAction::Repository(RepositoryAction::Amend(_)) => "Amending commit".to_owned(),
        ApplicationAction::Repository(RepositoryAction::UndoLastCommit(_)) => {
            "Undoing last commit".to_owned()
        }
        ApplicationAction::Repository(RepositoryAction::Revert(target)) => {
            format!("Reverting {}", &target.id[..target.id.len().min(7)])
        }
        ApplicationAction::Repository(RepositoryAction::RenameBranch(target)) => {
            format!("Renaming branch to {}", target.new_name)
        }
        ApplicationAction::AiCommit(_) => "AI commit".to_owned(),
        ApplicationAction::Update => "Update Diffo".to_owned(),
    }
}

impl Workbench {
    pub(super) fn cancel_clicked_command(&mut self, event: &Event, area: Rect) -> bool {
        let Event::Mouse(mouse) = event else {
            return false;
        };
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return false;
        }
        let (rows, hidden) = self.command_progress_rows();
        let Some(id) = command_at_position(
            CommandProgress {
                rows: &rows,
                hidden,
                animation_tick: self.command_animation_tick,
            },
            area,
            mouse.column,
            mouse.row,
        ) else {
            return false;
        };
        let changed = self.commands.cancel(id);
        if changed && self.commands.ai_commit_id().is_none() {
            self.diff.model.finish_ai_commit();
        }
        changed
    }

    pub(super) fn command_progress_rows(&self) -> (Vec<CommandProgressRow>, usize) {
        let visible_commands = diffo_ui::design::COMMAND_QUEUE_VISIBLE_ROWS;
        let total = self.commands.entries().count();
        let rows = self
            .commands
            .entries()
            .take(visible_commands)
            .map(|(id, label, state)| CommandProgressRow {
                id,
                label,
                state: match state {
                    CommandState::Queued => CommandRowState::Queued,
                    CommandState::Cancelling => CommandRowState::Cancelling,
                    CommandState::Running => CommandRowState::Active,
                },
            })
            .collect();
        (rows, total.saturating_sub(visible_commands))
    }

    pub fn take_application_command(&mut self, now: Instant) -> Option<ApplicationCommand> {
        let queued = self.commands.take_next()?;
        let action = match self.prepare_command_intent(&queued.intent) {
            Ok(action) => action,
            Err(detail) => {
                self.commands.fail_preparation();
                self.diff.model.finish_ai_commit();
                self.show_error("Queued command stopped", detail);
                return None;
            }
        };
        if let ApplicationAction::Repository(repository_action) = &action
            && !self
                .diff
                .model
                .activate_repository_action(repository_action.clone())
        {
            self.diff.model.cancel_operation(repository_action);
            self.commands.fail_preparation();
            self.diff.model.finish_ai_commit();
            self.show_error(
                "Queued command stopped",
                "The previous repository command has not finished",
            );
            return None;
        }
        let command = self.commands.activate(queued, action);
        self.last_prompt_id = None;
        self.command_progress = WorkbenchProgressState::Waiting {
            command_id: command.id,
            reveal_at: now + Duration::from_millis(150),
        };
        self.command_animation_tick = 0;
        self.request_redraw();
        Some(command)
    }

    fn prepare_command_intent(
        &mut self,
        intent: &CommandIntent,
    ) -> Result<ApplicationAction, &'static str> {
        let repository = match intent {
            CommandIntent::Repository(action) => Some(action.clone()),
            CommandIntent::ToggleStage(key) => self.diff.model.prepare_toggle_stage(key),
            CommandIntent::ToggleStageAll => self.diff.model.prepare_toggle_stage_all(),
            CommandIntent::StageAll => self.diff.model.prepare_stage_all(),
            CommandIntent::UnstageAll => self.diff.model.prepare_unstage_all(),
            CommandIntent::StageFile(path) => self.diff.model.prepare_stage_file(path.clone()),
            CommandIntent::UnstageFile(path) => self.diff.model.prepare_unstage_file(path.clone()),
            CommandIntent::Commit(draft) => self.diff.model.prepare_commit(draft),
            CommandIntent::AiCommit => {
                return AiCommitRequest::from_snapshot(&self.diff.model.snapshot)
                    .map(ApplicationAction::AiCommit)
                    .ok_or("There are no staged changes for the AI commit");
            }
            CommandIntent::Update => return Ok(ApplicationAction::Update),
        };
        repository
            .map(ApplicationAction::Repository)
            .ok_or("The repository no longer has the changes this command needs")
    }

    pub(super) fn enqueue_followup_intent(&mut self, message: &Message) -> bool {
        let intent = match message {
            Message::ToggleStageSelected => self
                .diff
                .model
                .selected
                .clone()
                .map(CommandIntent::ToggleStage),
            Message::ToggleStageAll => Some(CommandIntent::ToggleStageAll),
            Message::StageAll => Some(CommandIntent::StageAll),
            Message::UnstageAll => Some(CommandIntent::UnstageAll),
            Message::StageFile(path) => Some(CommandIntent::StageFile(path.clone())),
            Message::UnstageFile(path) => Some(CommandIntent::UnstageFile(path.clone())),
            Message::ExecuteCommit => Some(CommandIntent::Commit(
                self.diff.model.commit_message.clone(),
            )),
            Message::ExecuteSync => Some(CommandIntent::Repository(RepositoryAction::Sync)),
            Message::ExecuteSyncToRemote(remote) => Some(CommandIntent::Repository(
                RepositoryAction::SyncToRemote(remote.clone()),
            )),
            _ => return false,
        };
        if let Some(intent) = intent {
            if matches!(message, Message::ExecuteCommit) {
                self.close_modal();
            }
            self.commands.enqueue_intent(intent);
            self.request_redraw();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_start_one_at_a_time_in_fifo_order() {
        let mut queue = CommandQueue::new();
        let fetch = queue.enqueue(RepositoryAction::Fetch);
        let sync = queue.enqueue(RepositoryAction::Sync);

        let queued = queue.take_next().expect("fetch queued");
        assert_eq!(queued.id, fetch);
        let _ = queue.activate(
            queued,
            ApplicationAction::Repository(RepositoryAction::Fetch),
        );
        assert!(queue.take_next().is_none());
        assert_eq!(queue.queued_len(), 1);
        assert!(queue.acknowledge(fetch, CommandResult::Succeeded));
        assert_eq!(queue.take_next().map(|command| command.id), Some(sync));
    }

    #[test]
    fn queued_cancellation_removes_the_command_and_everything_after_it() {
        let mut queue = CommandQueue::new();
        let fetch = queue.enqueue(RepositoryAction::Fetch);
        let sync = queue.enqueue(RepositoryAction::Sync);
        let update = queue.enqueue_update();

        assert!(queue.cancel(sync));
        assert_eq!(queue.take_next().map(|command| command.id), Some(fetch));
        assert!(!queue.entries().any(|(id, _, _)| id == update));
    }

    #[test]
    fn running_cancellation_waits_for_acknowledgement() {
        let mut queue = CommandQueue::new();
        let fetch = queue.enqueue(RepositoryAction::Fetch);
        let sync = queue.enqueue(RepositoryAction::Sync);
        let queued = queue.take_next().expect("fetch queued");
        let running = queue.activate(
            queued,
            ApplicationAction::Repository(RepositoryAction::Fetch),
        );

        assert!(queue.cancel(fetch));
        assert!(running.cancellation.is_cancelled());
        assert_eq!(
            queue.active().map(|command| command.state),
            Some(CommandState::Cancelling)
        );
        assert!(queue.take_next().is_none());
        assert_eq!(queue.queued_len(), 0);

        assert!(queue.acknowledge(fetch, CommandResult::Cancelled));
        assert!(queue.take_next().is_none());
        assert!(!queue.entries().any(|(id, _, _)| id == sync));
    }

    #[test]
    fn update_uses_the_same_serial_queue_as_repository_commands() {
        let mut queue = CommandQueue::new();
        let fetch = queue.enqueue(RepositoryAction::Fetch);
        let update = queue.enqueue_update();

        let queued = queue.take_next().expect("fetch queued");
        assert_eq!(queued.id, fetch);
        let _ = queue.activate(
            queued,
            ApplicationAction::Repository(RepositoryAction::Fetch),
        );
        assert!(queue.acknowledge(fetch, CommandResult::Succeeded));
        let queued = queue.take_next().unwrap();
        assert_eq!(queued.id, update);
        let command = queue.activate(queued, ApplicationAction::Update);
        assert_eq!(command.action, ApplicationAction::Update);
    }

    #[test]
    fn failure_discards_every_waiting_command() {
        let mut queue = CommandQueue::new();
        let fetch = queue.enqueue(RepositoryAction::Fetch);
        queue.enqueue(RepositoryAction::Sync);
        queue.enqueue_update();
        let queued = queue.take_next().expect("fetch queued");
        let _ = queue.activate(
            queued,
            ApplicationAction::Repository(RepositoryAction::Fetch),
        );

        assert!(queue.acknowledge(fetch, CommandResult::Failed));

        assert!(!queue.has_work());
    }

    #[test]
    fn multi_step_commands_keep_the_goal_visible_while_the_phase_changes() {
        let mut queue = CommandQueue::new();
        let sync = queue.enqueue(RepositoryAction::Sync);
        let ai = queue.enqueue_ai_commit();
        assert_eq!(
            queue.entries().collect::<Vec<_>>(),
            vec![
                (sync, "Sync".to_owned(), CommandState::Queued),
                (ai, "AI commit".to_owned(), CommandState::Queued),
            ]
        );

        let queued = queue.take_next().expect("sync queued");
        let _ = queue.activate(
            queued,
            ApplicationAction::Repository(RepositoryAction::Sync),
        );
        assert_eq!(
            queue.entries().next(),
            Some((sync, "Sync — Fetching".to_owned(), CommandState::Running))
        );
        queue.active_mut().unwrap().phase = Some("Pushing".to_owned());
        assert_eq!(
            queue.entries().next(),
            Some((sync, "Sync — Pushing".to_owned(), CommandState::Running))
        );
    }
}
