use super::{
    ChangeArea, FileKey, Model, PathBuf, PendingFileAction, RepositoryAction, StageFileAction,
    UnstageFileAction, file_keys,
};

impl Model {
    #[must_use]
    pub fn stage_selected(&self) -> Option<RepositoryAction> {
        self.selected.as_ref().and_then(|key| {
            (key.area == ChangeArea::Unstaged).then(|| RepositoryAction::Stage(key.path.clone()))
        })
    }

    #[must_use]
    pub fn toggle_stage_selected(&mut self) -> Option<RepositoryAction> {
        if self.ai_commit_pending || self.pending_file_action.is_some() {
            return None;
        }
        let selected = self.selected.clone()?;
        self.prepare_toggle_stage(&selected)
    }

    pub(crate) fn prepare_toggle_stage(&mut self, requested: &FileKey) -> Option<RepositoryAction> {
        let keys = file_keys(&self.snapshot);
        let selected = keys
            .iter()
            .find(|key| *key == requested)
            .cloned()
            .or_else(|| {
                keys.iter()
                    .find(|key| key.path == requested.path && key.area != requested.area)
                    .cloned()
            })?;
        let action = match selected.area {
            ChangeArea::Unstaged => RepositoryAction::Stage(selected.path.clone()),
            ChangeArea::Staged => RepositoryAction::Unstage(selected.path.clone()),
        };
        let peers = keys
            .into_iter()
            .filter(|key| key.area == selected.area)
            .collect::<Vec<_>>();
        let next = peers
            .iter()
            .position(|key| key == &selected)
            .and_then(|index| (peers.len() > 1).then(|| peers[(index + 1) % peers.len()].clone()));
        self.pending_file_action = Some(match selected.area {
            ChangeArea::Unstaged => PendingFileAction::StageFile(StageFileAction {
                path: selected.path,
                next_unstaged: next,
            }),
            ChangeArea::Staged => PendingFileAction::UnstageFile(UnstageFileAction {
                path: selected.path,
                next_staged: next,
            }),
        });
        Some(action)
    }

    #[must_use]
    pub fn unstage_selected(&self) -> Option<RepositoryAction> {
        self.selected.as_ref().and_then(|key| {
            (key.area == ChangeArea::Staged).then(|| RepositoryAction::Unstage(key.path.clone()))
        })
    }

    #[must_use]
    pub fn toggle_stage_all(&self) -> Option<RepositoryAction> {
        if self.ai_commit_pending {
            return None;
        }
        self.prepare_toggle_stage_all()
    }

    pub(crate) fn prepare_toggle_stage_all(&self) -> Option<RepositoryAction> {
        if self
            .snapshot
            .files
            .iter()
            .any(|file| file.unstaged.is_some() || file.kind == diffo_core::ChangeKind::Untracked)
        {
            Some(RepositoryAction::StageAll)
        } else {
            self.snapshot
                .files
                .iter()
                .any(|file| file.staged.is_some())
                .then_some(RepositoryAction::UnstageAll)
        }
    }

    #[must_use]
    pub fn stage_all(&self) -> Option<RepositoryAction> {
        if self.ai_commit_pending {
            return None;
        }
        self.prepare_stage_all()
    }

    pub(crate) fn prepare_stage_all(&self) -> Option<RepositoryAction> {
        self.snapshot
            .files
            .iter()
            .any(|file| file.unstaged.is_some() || file.kind == diffo_core::ChangeKind::Untracked)
            .then_some(RepositoryAction::StageAll)
    }

    #[must_use]
    pub fn unstage_all(&self) -> Option<RepositoryAction> {
        if self.ai_commit_pending {
            return None;
        }
        self.prepare_unstage_all()
    }

    pub(crate) fn prepare_unstage_all(&self) -> Option<RepositoryAction> {
        self.snapshot
            .files
            .iter()
            .any(|file| file.staged.is_some())
            .then_some(RepositoryAction::UnstageAll)
    }

    #[must_use]
    pub fn stage_file(&self, path: PathBuf) -> Option<RepositoryAction> {
        if self.ai_commit_pending {
            return None;
        }
        self.prepare_stage_file(path)
    }

    pub(crate) fn prepare_stage_file(&self, path: PathBuf) -> Option<RepositoryAction> {
        self.snapshot
            .files
            .iter()
            .any(|file| {
                file.path == path
                    && (file.unstaged.is_some() || file.kind == diffo_core::ChangeKind::Untracked)
            })
            .then_some(RepositoryAction::Stage(path))
    }

    #[must_use]
    pub fn unstage_file(&self, path: PathBuf) -> Option<RepositoryAction> {
        if self.ai_commit_pending {
            return None;
        }
        self.prepare_unstage_file(path)
    }

    pub(crate) fn prepare_unstage_file(&self, path: PathBuf) -> Option<RepositoryAction> {
        self.snapshot
            .files
            .iter()
            .any(|file| file.path == path && file.staged.is_some())
            .then_some(RepositoryAction::Unstage(path))
    }
}
