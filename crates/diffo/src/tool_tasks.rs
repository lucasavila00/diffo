use std::sync::Arc;

use diffo_core::Repository;
use diffo_explorer::ExplorerWorker;
use diffo_workbench::{Workbench, WorkbenchTask, WorkbenchTaskResult};

pub(crate) struct ToolTasks {
    explorer: ExplorerWorker,
}

impl ToolTasks {
    pub(crate) fn start(repository: Arc<dyn Repository>) -> Self {
        Self {
            explorer: ExplorerWorker::start(repository),
        }
    }

    pub(crate) fn drain(&self, workbench: &mut Workbench) {
        while let Some(outcome) = self.explorer.try_recv() {
            workbench.accept_task_result(WorkbenchTaskResult::Explorer(outcome));
        }
        while let Some(task) = workbench.take_task() {
            match task {
                WorkbenchTask::Explorer(request) => self.explorer.submit(request),
            }
        }
    }
}
