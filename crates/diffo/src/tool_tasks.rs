use std::sync::Arc;

use diffo_app::explorer::ExplorerWorker;
use diffo_app::workbench::{Workbench, WorkbenchTask, WorkbenchTaskResult};
use diffo_core::Repository;

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
