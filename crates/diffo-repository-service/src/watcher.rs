use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
};

use anyhow::{Context, Result};
use diffo_core::RepositoryWatchPaths;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::worker::WorkerRequest;

pub(super) struct RepositoryWatcher {
    _watcher: RecommendedWatcher,
}

impl RepositoryWatcher {
    pub(super) fn start(
        paths: &RepositoryWatchPaths,
        requests: Sender<WorkerRequest>,
        refresh_pending: Arc<AtomicBool>,
        worktree_pending: Arc<AtomicBool>,
    ) -> Result<Self> {
        let worktree = paths.worktree.clone();
        let control_entry = worktree.join(".git");
        let git_metadata = paths.git_metadata.clone();
        let callback_requests = requests;
        let callback_pending = refresh_pending;
        let callback_worktree_pending = worktree_pending;
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
                Ok(event) => {
                    if event.paths.iter().any(|path| {
                        is_worktree_path(path, &worktree, &control_entry, &git_metadata)
                    }) {
                        callback_worktree_pending.store(true, Ordering::Release);
                    }
                    if !callback_pending.swap(true, Ordering::AcqRel) {
                        let _ = callback_requests.send(WorkerRequest::RefreshRequested);
                    }
                }
                Err(error) => {
                    let _ = callback_requests.send(WorkerRequest::WatchFailed(error.to_string()));
                }
            })
            .context("failed to create repository watcher")?;
        for path in std::iter::once(&paths.worktree).chain(paths.git_metadata.iter()) {
            watcher
                .watch(path, RecursiveMode::Recursive)
                .with_context(|| format!("failed to watch {}", path.display()))?;
        }
        Ok(Self { _watcher: watcher })
    }
}

fn is_worktree_path(
    path: &std::path::Path,
    worktree: &std::path::Path,
    control_entry: &std::path::Path,
    git_metadata: &[PathBuf],
) -> bool {
    path.starts_with(worktree)
        && !path.starts_with(control_entry)
        && !git_metadata
            .iter()
            .any(|metadata| metadata.starts_with(worktree) && path.starts_with(metadata))
}

#[cfg(test)]
mod tests {
    use super::is_worktree_path;
    use std::path::{Path, PathBuf};

    #[test]
    fn distinguishes_worktree_and_git_metadata_paths() {
        let worktree = Path::new("/repo");
        let control = Path::new("/repo/.git");
        let metadata = vec![PathBuf::from("/repo/.git"), PathBuf::from("/git/common")];

        assert!(is_worktree_path(
            Path::new("/repo/ignored/file.txt"),
            worktree,
            control,
            &metadata
        ));
        assert!(!is_worktree_path(
            Path::new("/repo/.git/index"),
            worktree,
            control,
            &metadata
        ));
        assert!(!is_worktree_path(
            Path::new("/git/common/refs/heads/main"),
            worktree,
            control,
            &metadata
        ));
    }
}
