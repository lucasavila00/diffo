use std::{path::PathBuf, sync::Arc};

use diffo_core::{HeadState, RepositorySnapshot};
use diffo_ui::terminal_safe_text;
use ratatui::text::Line;

use super::{HistoryActivity, HistoryTarget, ReviewSelection};

impl HistoryActivity {
    #[must_use]
    pub fn document_commits(&self) -> (Option<String>, Option<String>, Option<String>) {
        let requested = self
            .pending_selection
            .as_ref()
            .or(self.selection.as_ref())
            .and_then(selection_commit_id)
            .map(str::to_owned);
        let selected = self
            .selection
            .as_ref()
            .and_then(selection_commit_id)
            .map(str::to_owned);
        let displayed = self
            .reviewer
            .displayed_review_selection()
            .and_then(selection_commit_id)
            .map(str::to_owned);
        (requested, selected, displayed)
    }

    pub(super) fn document_files(&self) -> (Option<PathBuf>, Option<PathBuf>, Option<PathBuf>) {
        let requested = self
            .pending_selection
            .as_ref()
            .or(self.selection.as_ref())
            .and_then(selection_file_path);
        let selected = self.selection.as_ref().and_then(selection_file_path);
        let displayed = self
            .reviewer
            .displayed_review_selection()
            .and_then(selection_file_path);
        (requested, selected, displayed)
    }

    #[must_use]
    pub fn is_preparing(&self) -> bool {
        self.history_pending
            || self.patch_pending
            || self.file_pending
            || self.reviewer.is_preparing()
            || !self.queued.is_empty()
    }

    #[must_use]
    pub fn help_rows(&self) -> Vec<(String, &'static str)> {
        vec![
            ("j / k".to_owned(), "Previous / next commit"),
            ("h / l".to_owned(), "Previous / next file"),
            ("q / Esc / Ctrl+c".to_owned(), "Quit"),
        ]
        .into_iter()
        .chain(crate::diff::review_help_rows())
        .collect()
    }
}

pub(super) fn selection_commit_id(selection: &ReviewSelection) -> Option<&str> {
    match selection {
        ReviewSelection::File(_) => None,
        ReviewSelection::HistoryFile { commit_id, .. }
        | ReviewSelection::CompleteChange(commit_id) => Some(commit_id),
    }
}

pub(super) fn selection_target(selection: &ReviewSelection) -> Option<HistoryTarget> {
    match selection {
        ReviewSelection::File(_) | ReviewSelection::CompleteChange(_) => None,
        ReviewSelection::HistoryFile { path, .. } => Some(HistoryTarget::File(path.clone())),
    }
}

pub(super) fn split_file_patches(patch: &str) -> Vec<Arc<str>> {
    let mut starts = patch
        .match_indices("diff --git ")
        .filter_map(|(index, _)| {
            (index == 0 || patch.as_bytes()[index - 1] == b'\n').then_some(index)
        })
        .collect::<Vec<_>>();
    if starts.is_empty() {
        return (!patch.is_empty())
            .then(|| Arc::from(patch))
            .into_iter()
            .collect();
    }
    starts.push(patch.len());
    starts
        .windows(2)
        .map(|bounds| Arc::from(&patch[bounds[0]..bounds[1]]))
        .collect()
}

fn selection_file_path(selection: &ReviewSelection) -> Option<PathBuf> {
    match selection {
        ReviewSelection::HistoryFile { path, .. } => Some(path.clone()),
        ReviewSelection::File(_) | ReviewSelection::CompleteChange(_) => None,
    }
}

pub(super) fn commit_title(commit_id: &str, summary: &str) -> Line<'static> {
    let short = commit_id.get(..7).unwrap_or(commit_id);
    Line::raw(format!(" {short} · {} ", terminal_safe_text(summary)))
}

pub(super) fn snapshot_head(snapshot: &RepositorySnapshot) -> Option<String> {
    match &snapshot.head {
        HeadState::Named { commit, .. } | HeadState::Detached { commit } => Some(commit.clone()),
        HeadState::Unborn { .. } => None,
    }
}
