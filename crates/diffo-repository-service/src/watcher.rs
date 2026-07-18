use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
};

use anyhow::{Context, Result};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::worker::WorkerRequest;

pub(super) struct RepositoryWatcher {
    _watcher: RecommendedWatcher,
}

impl RepositoryWatcher {
    pub(super) fn start(
        paths: &[PathBuf],
        requests: Sender<WorkerRequest>,
        refresh_pending: Arc<AtomicBool>,
    ) -> Result<Self> {
        let callback_requests = requests;
        let callback_pending = refresh_pending;
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
                Ok(_) => {
                    if !callback_pending.swap(true, Ordering::AcqRel) {
                        let _ = callback_requests.send(WorkerRequest::RefreshRequested);
                    }
                }
                Err(error) => {
                    let _ = callback_requests.send(WorkerRequest::WatchFailed(error.to_string()));
                }
            })
            .context("failed to create repository watcher")?;
        for path in paths {
            watcher
                .watch(path, RecursiveMode::Recursive)
                .with_context(|| format!("failed to watch {}", path.display()))?;
        }
        Ok(Self { _watcher: watcher })
    }
}
