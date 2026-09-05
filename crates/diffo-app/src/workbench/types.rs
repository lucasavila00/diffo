use crate::diff::Message;
use crate::explorer::{ExplorerOutcome, ExplorerRequest};
use diffo_core::{ApplicationCommandId, PromptId};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Activity {
    #[default]
    Diff,
    Explorer,
    History,
}

pub(super) enum WorkbenchCommand {
    Diff(Message),
    Effect(WorkbenchEffect),
    Redraw,
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

pub enum WorkbenchTask {
    Explorer(ExplorerRequest),
}

pub enum WorkbenchTaskResult {
    Explorer(ExplorerOutcome),
}
