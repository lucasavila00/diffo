use super::{AccessMode, ChangeArea, Model, PathBuf, RepositoryAction, file_keys};

impl Model {
    #[must_use]
    pub fn stage_selected(&self) -> Option<RepositoryAction> {
        if self.access_mode == AccessMode::ReadOnly {
            return None;
        }
        self.selected.as_ref().and_then(|key| {
            (key.area == ChangeArea::Unstaged).then(|| RepositoryAction::Stage(key.path.clone()))
        })
    }

    #[must_use]
    pub fn toggle_stage_selected(&mut self) -> Option<RepositoryAction> {
        let selected = self.selected.clone()?;
        let action = match selected.area {
            ChangeArea::Unstaged => self.stage_selected(),
            ChangeArea::Staged => self.unstage_selected(),
        };
        if action.is_some() {
            let peers = file_keys(&self.snapshot)
                .into_iter()
                .filter(|key| key.area == selected.area)
                .collect::<Vec<_>>();
            self.selection_after_action =
                peers
                    .iter()
                    .position(|key| key == &selected)
                    .and_then(|index| {
                        (peers.len() > 1).then(|| peers[(index + 1) % peers.len()].clone())
                    });
        }
        action
    }

    #[must_use]
    pub fn unstage_selected(&self) -> Option<RepositoryAction> {
        if self.access_mode == AccessMode::ReadOnly {
            return None;
        }
        self.selected.as_ref().and_then(|key| {
            (key.area == ChangeArea::Staged).then(|| RepositoryAction::Unstage(key.path.clone()))
        })
    }

    #[must_use]
    pub fn toggle_stage_all(&self) -> Option<RepositoryAction> {
        if self.access_mode == AccessMode::ReadOnly {
            return None;
        }
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
        (self.access_mode == AccessMode::ReadWrite
            && self.snapshot.files.iter().any(|file| {
                file.unstaged.is_some() || file.kind == diffo_core::ChangeKind::Untracked
            }))
        .then_some(RepositoryAction::StageAll)
    }

    #[must_use]
    pub fn unstage_all(&self) -> Option<RepositoryAction> {
        (self.access_mode == AccessMode::ReadWrite
            && self.snapshot.files.iter().any(|file| file.staged.is_some()))
        .then_some(RepositoryAction::UnstageAll)
    }

    #[must_use]
    pub fn stage_file(&self, path: PathBuf) -> Option<RepositoryAction> {
        (self.access_mode == AccessMode::ReadWrite
            && self.snapshot.files.iter().any(|file| {
                file.path == path
                    && (file.unstaged.is_some() || file.kind == diffo_core::ChangeKind::Untracked)
            }))
        .then_some(RepositoryAction::Stage(path))
    }

    #[must_use]
    pub fn unstage_file(&self, path: PathBuf) -> Option<RepositoryAction> {
        (self.access_mode == AccessMode::ReadWrite
            && self
                .snapshot
                .files
                .iter()
                .any(|file| file.path == path && file.staged.is_some()))
        .then_some(RepositoryAction::Unstage(path))
    }
}
