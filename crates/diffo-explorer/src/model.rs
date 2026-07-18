//! Pure Explorer tree and viewer state.

use std::{
    collections::{BTreeSet, HashMap},
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
    pub(crate) coverage: Vec<LineRange>,
    pub(crate) syntax_eligible: bool,
    pub(crate) message: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TreeEntry {
    pub(crate) path: PathBuf,
    pub(crate) depth: usize,
    pub(crate) directory: bool,
    pub(crate) status: Option<ChangeKind>,
}

pub(crate) struct ExplorerModel {
    snapshot: RepositorySnapshot,
    paths: Vec<PathBuf>,
    pub(crate) entries: Vec<TreeEntry>,
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
            entries: Vec::new(),
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
        let statuses = self
            .snapshot
            .files
            .iter()
            .map(|file| (file.path.clone(), file.kind))
            .collect::<HashMap<_, _>>();
        let directories = self.directory_paths();
        let mut entries = directories
            .into_iter()
            .map(|path| TreeEntry {
                depth: path.components().count().saturating_sub(1),
                path,
                directory: true,
                status: None,
            })
            .chain(self.paths.iter().cloned().map(|path| TreeEntry {
                depth: path.components().count().saturating_sub(1),
                status: statuses.get(&path).copied(),
                path,
                directory: false,
            }))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        self.entries = entries;
    }

    fn directory_paths(&self) -> BTreeSet<PathBuf> {
        let mut directories = BTreeSet::new();
        for path in &self.paths {
            let mut parent = path.parent();
            while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
                directories.insert(path.to_path_buf());
                parent = path.parent();
            }
        }
        directories
    }

    pub(crate) fn entry(&self, path: &Path) -> Option<&TreeEntry> {
        self.entries.iter().find(|entry| entry.path == path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diffo_core::{FileDiff, FileState};

    #[test]
    fn builds_hierarchy_with_merged_status() {
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

        assert_eq!(model.entries.len(), 4);
        assert_eq!(
            model.entry(Path::new("src/changed.rs")).unwrap().status,
            Some(ChangeKind::Modified)
        );
        assert_eq!(
            model.entry(Path::new("src/unchanged.rs")).unwrap().status,
            None
        );
        assert!(model.entry(Path::new("src")).unwrap().directory);
    }
}
