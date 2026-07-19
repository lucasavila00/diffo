//! Pure Explorer tree and viewer state.

use std::{
    collections::{BTreeMap, HashMap},
    ffi::OsString,
    path::{Path, PathBuf},
};

use diffo_core::{ChangeKind, RepositorySnapshot};
use diffo_highlight::{HighlightedLine, LineRange};
use ratatui::text::Line;

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
    pub(crate) title: Box<Line<'static>>,
    pub(crate) lines: Vec<String>,
    pub(crate) markers: HashMap<usize, GutterMarker>,
    pub(crate) highlighted: HashMap<u32, HighlightedLine>,
    pub(crate) coverage: Vec<LineRange>,
    pub(crate) syntax_eligible: bool,
    pub(crate) message: Option<String>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum EntryId {
    Directory(PathBuf),
    File(PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TreeEntry {
    pub(crate) id: EntryId,
    pub(crate) status: Option<ChangeKind>,
    pub(crate) children: Vec<Self>,
}

impl TreeEntry {
    pub(crate) fn path(&self) -> &Path {
        match &self.id {
            EntryId::Directory(path) | EntryId::File(path) => path,
        }
    }

    pub(crate) const fn directory(&self) -> bool {
        matches!(self.id, EntryId::Directory(_))
    }
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
        let mut root = TreeBuilder::default();
        for path in &self.paths {
            root.insert(path, statuses.get(path).copied());
        }
        self.entries = root.finish(Path::new(""));
    }

    pub(crate) fn file_entry(&self, path: &Path) -> Option<&TreeEntry> {
        find_entry(&self.entries, &EntryId::File(path.to_path_buf()))
    }
}

#[derive(Default)]
struct TreeBuilder {
    directories: BTreeMap<OsString, Self>,
    files: BTreeMap<OsString, Option<ChangeKind>>,
}

impl TreeBuilder {
    fn insert(&mut self, path: &Path, status: Option<ChangeKind>) {
        let mut components = path.components().peekable();
        let mut directory = self;
        while let Some(component) = components.next() {
            let name = component.as_os_str().to_os_string();
            if components.peek().is_none() {
                directory.files.insert(name, status);
            } else {
                directory = directory.directories.entry(name).or_default();
            }
        }
    }

    fn finish(self, parent: &Path) -> Vec<TreeEntry> {
        let mut entries = self
            .directories
            .into_iter()
            .map(|(name, directory)| {
                let path = parent.join(name);
                let children = directory.finish(&path);
                TreeEntry {
                    id: EntryId::Directory(path),
                    status: children
                        .iter()
                        .filter_map(|entry| entry.status)
                        .max_by_key(|status| status_priority(*status)),
                    children,
                }
            })
            .chain(self.files.into_iter().map(|(name, status)| TreeEntry {
                id: EntryId::File(parent.join(name)),
                status,
                children: Vec::new(),
            }))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left.path()
                .file_name()
                .cmp(&right.path().file_name())
                .then_with(|| right.directory().cmp(&left.directory()))
        });
        entries
    }
}

const fn status_priority(status: ChangeKind) -> u8 {
    match status {
        ChangeKind::Conflicted => 5,
        ChangeKind::Deleted => 4,
        ChangeKind::Modified => 3,
        ChangeKind::Renamed | ChangeKind::Copied => 2,
        ChangeKind::Added | ChangeKind::Untracked => 1,
    }
}

fn find_entry<'a>(entries: &'a [TreeEntry], id: &EntryId) -> Option<&'a TreeEntry> {
    entries.iter().find_map(|entry| {
        if &entry.id == id {
            Some(entry)
        } else {
            find_entry(&entry.children, id)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use diffo_core::{FileDiff, FileState};

    #[test]
    fn builds_hierarchy_bubbles_status_and_clears_on_refresh() {
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

        assert_eq!(model.entries.len(), 2);
        assert_eq!(
            model
                .file_entry(Path::new("src/changed.rs"))
                .unwrap()
                .status,
            Some(ChangeKind::Modified)
        );
        assert_eq!(
            model
                .file_entry(Path::new("src/unchanged.rs"))
                .unwrap()
                .status,
            None
        );
        assert!(matches!(model.entries[1].id, EntryId::Directory(_)));
        assert_eq!(model.entries[1].status, Some(ChangeKind::Modified));

        assert!(model.repository_changed(RepositorySnapshot::default()));
        assert_eq!(model.entries[1].status, None);
        assert_eq!(
            model
                .file_entry(Path::new("src/changed.rs"))
                .unwrap()
                .status,
            None
        );
    }

    #[test]
    fn nested_directories_bubble_the_strongest_descendant_status() {
        let mut tree = TreeBuilder::default();
        for (path, status) in [
            ("src/conflicted.rs", ChangeKind::Conflicted),
            ("src/deleted.rs", ChangeKind::Deleted),
            ("src/nested/deleted.rs", ChangeKind::Deleted),
            ("src/nested/modified.rs", ChangeKind::Modified),
            ("src/low/modified.rs", ChangeKind::Modified),
            ("src/low/renamed.rs", ChangeKind::Renamed),
            ("src/low/info/copied.rs", ChangeKind::Copied),
            ("src/low/info/added.rs", ChangeKind::Added),
        ] {
            tree.insert(Path::new(path), Some(status));
        }
        let entries = tree.finish(Path::new(""));

        assert_eq!(entries[0].status, Some(ChangeKind::Conflicted));
        assert_eq!(
            find_entry(&entries, &EntryId::Directory("src/nested".into()))
                .unwrap()
                .status,
            Some(ChangeKind::Deleted)
        );
        assert_eq!(
            find_entry(&entries, &EntryId::Directory("src/low".into()))
                .unwrap()
                .status,
            Some(ChangeKind::Modified)
        );
        assert!(matches!(
            find_entry(&entries, &EntryId::Directory("src/low/info".into()))
                .unwrap()
                .status,
            Some(ChangeKind::Renamed | ChangeKind::Copied)
        ));
    }

    #[test]
    fn preserves_a_file_and_directory_with_the_same_path() {
        let snapshot = RepositorySnapshot {
            files: vec![FileState {
                path: PathBuf::from("foo"),
                old_path: None,
                kind: ChangeKind::Deleted,
                staged: None,
                unstaged: Some(FileDiff {
                    text: String::new(),
                }),
            }],
            ..RepositorySnapshot::default()
        };
        let mut model = ExplorerModel::new(snapshot);
        model.install_paths(vec![PathBuf::from("foo/bar.rs")]);

        assert_eq!(model.entries.len(), 2);
        assert_eq!(
            model.entries[0].id,
            EntryId::Directory(PathBuf::from("foo"))
        );
        assert_eq!(model.entries[0].children.len(), 1);
        assert_eq!(model.entries[1].id, EntryId::File(PathBuf::from("foo")));
        assert_eq!(model.entries[1].status, Some(ChangeKind::Deleted));
    }
}
