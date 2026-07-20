use std::collections::VecDeque;

use diffo_core::{ApplicationCommandId, CancellationHandle, RepositoryAction};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationAction {
    Repository(RepositoryAction),
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

#[derive(Default)]
pub struct CommandQueue {
    queued: VecDeque<ApplicationCommand>,
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
        self.enqueue_action(ApplicationAction::Repository(action))
    }

    pub fn enqueue_update(&mut self) -> ApplicationCommandId {
        self.enqueue_action(ApplicationAction::Update)
    }

    fn enqueue_action(&mut self, action: ApplicationAction) -> ApplicationCommandId {
        let id = ApplicationCommandId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.queued.push_back(ApplicationCommand {
            id,
            label: command_label(&action),
            action,
            cancellation: CancellationHandle::default(),
            state: CommandState::Queued,
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
    pub fn queued_len(&self) -> usize {
        self.queued.len()
    }

    pub fn start_next(&mut self) -> Option<ApplicationCommand> {
        if self.active.is_some() {
            return None;
        }
        let mut command = self.queued.pop_front()?;
        command.state = CommandState::Running;
        self.active = Some(command.clone());
        Some(command)
    }

    pub fn cancel(&mut self, id: ApplicationCommandId) -> bool {
        if let Some(command) = self.active.as_mut().filter(|command| command.id == id) {
            command.cancellation.cancel();
            command.state = CommandState::Cancelling;
            return true;
        }
        let original_len = self.queued.len();
        self.queued.retain(|command| command.id != id);
        self.queued.len() != original_len
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
        Some(command)
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
        ApplicationAction::Repository(RepositoryAction::Fetch | RepositoryAction::Sync) => {
            "Fetching".to_owned()
        }
        ApplicationAction::Repository(RepositoryAction::Commit(_)) => "Committing".to_owned(),
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

        assert_eq!(queue.start_next().map(|command| command.id), Some(fetch));
        assert!(queue.start_next().is_none());
        assert_eq!(queue.queued_len(), 1);
        assert_eq!(
            queue
                .acknowledge(fetch, CommandResult::Succeeded)
                .map(|command| command.state),
            Some(CommandState::Finished(CommandResult::Succeeded))
        );
        assert_eq!(queue.start_next().map(|command| command.id), Some(sync));
    }

    #[test]
    fn queued_cancellation_removes_the_command_immediately() {
        let mut queue = CommandQueue::new();
        let fetch = queue.enqueue(RepositoryAction::Fetch);
        let sync = queue.enqueue(RepositoryAction::Sync);

        assert!(queue.cancel(fetch));
        assert_eq!(queue.start_next().map(|command| command.id), Some(sync));
    }

    #[test]
    fn running_cancellation_waits_for_acknowledgement() {
        let mut queue = CommandQueue::new();
        let fetch = queue.enqueue(RepositoryAction::Fetch);
        let sync = queue.enqueue(RepositoryAction::Sync);
        let running = queue.start_next().expect("fetch starts");

        assert!(queue.cancel(fetch));
        assert!(running.cancellation.is_cancelled());
        assert_eq!(
            queue.active().map(|command| command.state),
            Some(CommandState::Cancelling)
        );
        assert!(queue.start_next().is_none());
        assert_eq!(queue.queued_len(), 1);

        queue
            .acknowledge(fetch, CommandResult::Cancelled)
            .expect("cancellation acknowledged");
        assert_eq!(queue.start_next().map(|command| command.id), Some(sync));
    }

    #[test]
    fn update_uses_the_same_serial_queue_as_repository_commands() {
        let mut queue = CommandQueue::new();
        let fetch = queue.enqueue(RepositoryAction::Fetch);
        let update = queue.enqueue_update();

        assert_eq!(queue.start_next().map(|command| command.id), Some(fetch));
        queue.acknowledge(fetch, CommandResult::Succeeded).unwrap();
        let command = queue.start_next().unwrap();
        assert_eq!(command.id, update);
        assert_eq!(command.action, ApplicationAction::Update);
    }
}
