use diffo_core::ApplicationCommandId;

use crate::review::{ReviewCodexOutcome, ReviewCodexTaskResult, ReviewEvent, ReviewProgress};

use super::{ApplicationAction, CommandIntent, CommandResult, CommandState, Workbench};

impl Workbench {
    pub fn accept_review_progress(&mut self, id: ApplicationCommandId, progress: ReviewProgress) {
        let running = self.commands.active().is_some_and(|command| {
            command.id == id
                && command.state == CommandState::Running
                && matches!(&command.action, ApplicationAction::AiReview(_))
        });
        let phase = progress.command_phase();
        if !running || !self.review.generation_progress(id, progress) {
            return;
        }
        if let Some(command) = self.commands.active_mut() {
            command.phase = Some(phase);
        }
        self.request_redraw();
    }

    pub fn accept_review_codex_result(&mut self, result: ReviewCodexTaskResult) {
        let id = result.id;
        let complete = result.complete;
        let cancelling = self
            .commands
            .active()
            .is_some_and(|command| command.id == id && command.state == CommandState::Cancelling);
        let result = if cancelling {
            ReviewCodexTaskResult {
                outcome: ReviewCodexOutcome::Cancelled,
                ..result
            }
        } else {
            result
        };
        let command_result = match &result.outcome {
            ReviewCodexOutcome::Generated(_) => CommandResult::Succeeded,
            ReviewCodexOutcome::Failed(_) => CommandResult::Failed,
            ReviewCodexOutcome::Cancelled => CommandResult::Cancelled,
        };
        if !self.review.accept(result) {
            return;
        }
        if complete {
            let _ = self.commands.acknowledge(id, command_result);
            self.finish_command_progress(id);
        }
        self.request_redraw();
    }

    pub(super) fn handle_review_event(&mut self, event: Option<ReviewEvent>) -> bool {
        let Some(event) = event else { return false };
        match event {
            ReviewEvent::Generate(request) => {
                if self.commands.has_work() {
                    self.review.generation_rejected(
                        "Finish the current command before starting an AI review.",
                    );
                } else {
                    let id = self
                        .commands
                        .enqueue_intent(CommandIntent::AiReview(request.clone()));
                    self.review.generation_queued(id, request);
                }
            }
            ReviewEvent::Cancel(id) => {
                let _ = self.cancel_application_command(id);
            }
            ReviewEvent::ToggleStage(file) => {
                self.commands
                    .enqueue_intent(CommandIntent::ToggleStage(file));
            }
            ReviewEvent::AiCommit => self.request_ai_commit(),
            ReviewEvent::Redraw => {}
        }
        true
    }
}
