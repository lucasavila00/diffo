use std::collections::VecDeque;
use std::path::PathBuf;

use diffo_core::{ApplicationCommandId, CancellationHandle, RepositoryAction};

use crate::diff::FileKey;

use super::AiCommitRequest;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationAction {
    Repository(RepositoryAction),
    AiCommit(AiCommitRequest),
    Update,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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
    Sync,
    SyncToRemote(String),
    Update,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandState {
    Queued,
    Running,
    Cancelling,
    Finished(CommandResult),
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
    pub label: String,
    pub cancellation: CancellationHandle,
    pub state: CommandState,
}

#[derive(Clone, Debug)]
pub(crate) struct QueuedCommand {
    pub id: ApplicationCommandId,
    pub intent: CommandIntent,
    pub label: String,
    pub cancellation: CancellationHandle,
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
        self.queued.push_back(QueuedCommand {
            id,
            label: intent_label(&intent),
            intent,
            cancellation: CancellationHandle::default(),
        });
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
                    .find(|command| command.intent == CommandIntent::AiCommit)
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
                CommandIntent::Sync
                    | CommandIntent::SyncToRemote(_)
                    | CommandIntent::Repository(
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

    pub fn entries(&self) -> impl Iterator<Item = (ApplicationCommandId, &str, CommandState)> {
        self.active
            .iter()
            .map(|command| (command.id, command.label.as_str(), command.state))
            .chain(
                self.queued
                    .iter()
                    .map(|command| (command.id, command.label.as_str(), CommandState::Queued)),
            )
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
        let command = ApplicationCommand {
            id: queued.id,
            label: command_label(&action),
            action,
            cancellation: queued.cancellation,
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

    pub fn cancel_all(&mut self) -> bool {
        let changed = self.has_work();
        self.queued.clear();
        if let Some(command) = self.active.as_mut() {
            command.cancellation.cancel();
            command.state = CommandState::Cancelling;
        }
        changed
    }

    pub(crate) fn fail_preparation(&mut self) {
        self.queued.clear();
    }

    pub fn acknowledge(
        &mut self,
        id: ApplicationCommandId,
        result: CommandResult,
    ) -> Option<ApplicationCommand> {
        if !self.active.as_ref().is_some_and(|command| command.id == id) {
            return None;
        }
        let mut command = self.active.take()?;
        command.state = CommandState::Finished(result);
        if result != CommandResult::Succeeded {
            self.queued.clear();
        }
        Some(command)
    }
}

fn intent_label(intent: &CommandIntent) -> String {
    match intent {
        CommandIntent::Repository(action) => {
            command_label(&ApplicationAction::Repository(action.clone()))
        }
        CommandIntent::ToggleStage(_) => "Stage / unstage file".to_owned(),
        CommandIntent::ToggleStageAll => "Stage / unstage all".to_owned(),
        CommandIntent::StageAll => "Stage all".to_owned(),
        CommandIntent::UnstageAll => "Unstage all".to_owned(),
        CommandIntent::StageFile(_) => "Stage file".to_owned(),
        CommandIntent::UnstageFile(_) => "Unstage file".to_owned(),
        CommandIntent::Commit(_) => "Commit".to_owned(),
        CommandIntent::AiCommit => "AI commit".to_owned(),
        CommandIntent::Sync | CommandIntent::SyncToRemote(_) => "Sync".to_owned(),
        CommandIntent::Update => "Update Diffo".to_owned(),
    }
}

fn command_label(action: &ApplicationAction) -> String {
    match action {
        ApplicationAction::Repository(RepositoryAction::Stage(_) | RepositoryAction::StageAll) => {
            "Staging".to_owned()
        }
        ApplicationAction::Repository(
            RepositoryAction::Unstage(_) | RepositoryAction::UnstageAll,
        ) => "Unstaging".to_owned(),
        ApplicationAction::Repository(
            RepositoryAction::Fetch | RepositoryAction::Sync | RepositoryAction::SyncToRemote(_),
        ) => "Fetching".to_owned(),
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
        ApplicationAction::AiCommit(_) => "Writing commit message".to_owned(),
        ApplicationAction::Update => "Updating Diffo".to_owned(),
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
        assert_eq!(
            queue
                .acknowledge(fetch, CommandResult::Succeeded)
                .map(|command| command.state),
            Some(CommandState::Finished(CommandResult::Succeeded))
        );
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

        queue
            .acknowledge(fetch, CommandResult::Cancelled)
            .expect("cancellation acknowledged");
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
        queue.acknowledge(fetch, CommandResult::Succeeded).unwrap();
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

        queue.acknowledge(fetch, CommandResult::Failed).unwrap();

        assert!(!queue.has_work());
    }
}
