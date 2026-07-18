//! Pure Explorer tree and viewer state.

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
};

use diffo_core::{ChangeKind, RepositorySnapshot};
use diffo_highlight::{HighlightedLine, LineRange};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GutterMarker {
    Added,
    Modified,
    Deleted,
    Conflict,
}

#[derive(Clone, Debug)]
pub struct Viewer {
    pub(crate) path: PathBuf,
    pub(crate) lines: Vec<String>,
    pub(crate) markers: HashMap<usize, GutterMarker>,
    pub(crate) highlighted: HashMap<u32, HighlightedLine>,
    pub(crate) coverage: Option<LineRange>,
    pub(crate) syntax_eligible: bool,
    pub(crate) message: Option<String>,
    pub(crate) maximum_width: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TreeEntry {
    pub(crate) path: PathBuf,
    pub(crate) depth: usize,
    pub(crate) directory: bool,
    pub(crate) expanded: bool,
    pub(crate) status: Option<ChangeKind>,
}

pub(crate) struct ExplorerModel {
    snapshot: RepositorySnapshot,
    paths: Vec<PathBuf>,
    expanded: HashSet<PathBuf>,
    pub(crate) visible: Vec<TreeEntry>,
    pub(crate) selected: usize,
    pub(crate) tree_scroll: usize,
    pub(crate) viewer_scroll: usize,
    pub(crate) viewer_horizontal_scroll: usize,
    pub(crate) viewer: Option<Viewer>,
    pub(crate) error: Option<String>,
}

impl ExplorerModel {
    pub(crate) fn new(snapshot: RepositorySnapshot) -> Self {
        Self {
            snapshot,
            paths: Vec::new(),
            expanded: HashSet::new(),
            visible: Vec::new(),
            selected: 0,
            tree_scroll: 0,
            viewer_scroll: 0,
            viewer_horizontal_scroll: 0,
            viewer: None,
            error: None,
        }
    }

    pub(crate) fn repository_changed(&mut self, snapshot: RepositorySnapshot) -> bool {
        if self.snapshot == snapshot {
            return false;
        }
        self.snapshot = snapshot;
        self.rebuild();
        true
    }

    pub(crate) fn install_paths(&mut self, mut paths: Vec<PathBuf>) {
        paths.extend(self.snapshot.files.iter().map(|file| file.path.clone()));
        paths.sort();
        paths.dedup();
        self.paths = paths;
        self.rebuild();
    }

    fn rebuild(&mut self) {
        let selected_path = self.selected_entry().map(|entry| entry.path.clone());
        let statuses = self
            .snapshot
            .files
            .iter()
            .map(|file| (file.path.clone(), file.kind))
            .collect::<HashMap<_, _>>();
        let mut directories = BTreeSet::new();
        for path in &self.paths {
            let mut parent = path.parent();
            while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
                directories.insert(path.to_path_buf());
                parent = path.parent();
            }
        }
        let mut entries = directories
            .into_iter()
            .map(|path| TreeEntry {
                depth: path.components().count().saturating_sub(1),
                expanded: self.expanded.contains(&path),
                path,
                directory: true,
                status: None,
            })
            .chain(self.paths.iter().cloned().map(|path| TreeEntry {
                depth: path.components().count().saturating_sub(1),
                status: statuses.get(&path).copied(),
                path,
                directory: false,
                expanded: false,
            }))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        self.visible = entries
            .into_iter()
            .filter(|entry| self.ancestors_expanded(&entry.path))
            .collect();
        self.selected = selected_path
            .as_ref()
            .and_then(|path| self.visible.iter().position(|entry| &entry.path == path))
            .unwrap_or_else(|| self.selected.min(self.visible.len().saturating_sub(1)));
    }

    fn ancestors_expanded(&self, path: &Path) -> bool {
        let mut parent = path.parent();
        while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
            if !self.expanded.contains(path) {
                return false;
            }
            parent = path.parent();
        }
        true
    }

    pub(crate) fn selected_entry(&self) -> Option<&TreeEntry> {
        self.visible.get(self.selected)
    }

    pub(crate) fn selected_file(&self) -> Option<&Path> {
        self.selected_entry()
            .filter(|entry| !entry.directory)
            .map(|entry| entry.path.as_path())
    }

    pub(crate) fn select_by(&mut self, amount: i64) {
        if amount < 0 {
            self.selected = self
                .selected
                .saturating_sub(usize::try_from(amount.unsigned_abs()).unwrap_or(usize::MAX));
        } else {
            self.selected = self
                .selected
                .saturating_add(usize::try_from(amount).unwrap_or(usize::MAX))
                .min(self.visible.len().saturating_sub(1));
        }
    }

    pub(crate) fn select(&mut self, index: usize) {
        self.selected = index.min(self.visible.len().saturating_sub(1));
    }

    pub(crate) fn toggle_selected_directory(&mut self) {
        let Some(entry) = self.selected_entry().filter(|entry| entry.directory) else {
            return;
        };
        let path = entry.path.clone();
        if !self.expanded.remove(&path) {
            self.expanded.insert(path);
        }
        self.rebuild();
    }

    pub(crate) fn ensure_tree_selection_visible(&mut self, rows: usize) {
        if rows == 0 {
            self.tree_scroll = 0;
        } else if self.selected < self.tree_scroll {
            self.tree_scroll = self.selected;
        } else if self.selected >= self.tree_scroll.saturating_add(rows) {
            self.tree_scroll = self.selected.saturating_add(1).saturating_sub(rows);
        }
        self.tree_scroll = self
            .tree_scroll
            .min(self.visible.len().saturating_sub(rows));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diffo_core::{FileDiff, FileState};

    #[test]
    fn builds_and_expands_tree_with_merged_status() {
        let snapshot = RepositorySnapshot {
            files: vec![FileState {
                path: PathBuf::from("src/changed.rs"),
                old_path: None,
                kind: ChangeKind::Modified,
                staged: None,
                unstaged: Some(FileDiff {
                    text: String::new(),
                }),
            }],
            ..RepositorySnapshot::default()
        };
        let mut model = ExplorerModel::new(snapshot);
        model.install_paths(vec![
            PathBuf::from("README.md"),
            PathBuf::from("src/changed.rs"),
            PathBuf::from("src/unchanged.rs"),
        ]);

        assert_eq!(model.visible.len(), 2);
        model.select(1);
        model.toggle_selected_directory();

        assert_eq!(model.visible.len(), 4);
        assert_eq!(model.visible[2].status, Some(ChangeKind::Modified));
        assert_eq!(model.visible[3].status, None);
    }
}
