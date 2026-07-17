use std::{
    cell::RefCell,
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::{AccessMode, Repository, RepositoryAction, RepositorySnapshot, RepositorySource};

pub struct FixtureRepositorySource {
    path: PathBuf,
}

impl FixtureRepositorySource {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl RepositorySource for FixtureRepositorySource {
    fn snapshot(&self) -> Result<RepositorySnapshot> {
        let contents = fs::read_to_string(&self.path).with_context(|| {
            format!(
                "failed to read mock repository state from {}",
                self.path.display()
            )
        })?;

        ron::from_str(&contents).with_context(|| {
            format!(
                "failed to parse mock repository state from {}",
                self.path.display()
            )
        })
    }
}

impl Repository for FixtureRepositorySource {
    fn access_mode(&self) -> AccessMode {
        AccessMode::ReadOnly
    }

    fn apply(&self, _action: &RepositoryAction) -> Result<()> {
        bail!("mock repository is read-only")
    }
}

pub struct MutableFixtureRepository {
    snapshot: RefCell<RepositorySnapshot>,
    untracked_paths: HashSet<PathBuf>,
}

impl MutableFixtureRepository {
    /// Load a mutable in-memory repository from a structured fixture.
    ///
    /// # Errors
    ///
    /// Returns an error when the fixture cannot be read or parsed.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let snapshot = FixtureRepositorySource::new(path).snapshot()?;
        Ok(Self::from_snapshot(snapshot))
    }

    /// Load a mutable fixture and append generated large-file changes.
    ///
    /// # Errors
    ///
    /// Returns an error when the base fixture cannot be read or parsed.
    pub fn new_with_large_files(path: impl Into<PathBuf>) -> Result<Self> {
        let mut snapshot = FixtureRepositorySource::new(path).snapshot()?;
        append_large_files(&mut snapshot, 100_000, 25_000, 100_000);
        Ok(Self::from_snapshot(snapshot))
    }

    fn from_snapshot(snapshot: RepositorySnapshot) -> Self {
        let untracked_paths = snapshot
            .files
            .iter()
            .filter(|file| file.kind == crate::ChangeKind::Untracked)
            .map(|file| file.path.clone())
            .collect();
        Self {
            snapshot: RefCell::new(snapshot),
            untracked_paths,
        }
    }

    fn stage(&self, path: &Path) -> Result<()> {
        let mut snapshot = self.snapshot.borrow_mut();
        let file = snapshot
            .files
            .iter_mut()
            .find(|file| file.path == path)
            .with_context(|| format!("mock repository has no file {}", path.display()))?;
        let unstaged = file
            .unstaged
            .take()
            .with_context(|| format!("{} has no unstaged changes", path.display()))?;
        file.staged = Some(unstaged);
        if file.kind == crate::ChangeKind::Untracked {
            file.kind = crate::ChangeKind::Added;
        }
        Ok(())
    }

    fn unstage(&self, path: &Path) -> Result<()> {
        let mut snapshot = self.snapshot.borrow_mut();
        let file = snapshot
            .files
            .iter_mut()
            .find(|file| file.path == path)
            .with_context(|| format!("mock repository has no file {}", path.display()))?;
        let staged = file
            .staged
            .take()
            .with_context(|| format!("{} has no staged changes", path.display()))?;
        if file.unstaged.is_none() {
            file.unstaged = Some(staged);
        }
        if self.untracked_paths.contains(path) {
            file.kind = crate::ChangeKind::Untracked;
        }
        Ok(())
    }
}

fn append_large_files(
    snapshot: &mut RepositorySnapshot,
    rust_lines: usize,
    json_items: usize,
    long_line_bytes: usize,
) {
    snapshot.files.push(crate::FileState {
        path: PathBuf::from("generated/huge.rs"),
        old_path: None,
        kind: crate::ChangeKind::Untracked,
        staged: None,
        unstaged: Some(crate::FileDiff {
            text: added_rust_patch(rust_lines),
        }),
    });
    snapshot.files.push(crate::FileState {
        path: PathBuf::from("generated/large.json"),
        old_path: None,
        kind: crate::ChangeKind::Untracked,
        staged: None,
        unstaged: Some(crate::FileDiff {
            text: added_json_patch(json_items),
        }),
    });
    snapshot.files.push(crate::FileState {
        path: PathBuf::from("generated/long-line.txt"),
        old_path: None,
        kind: crate::ChangeKind::Modified,
        staged: None,
        unstaged: Some(crate::FileDiff {
            text: format!(
                "diff --git a/generated/long-line.txt b/generated/long-line.txt\n--- a/generated/long-line.txt\n+++ b/generated/long-line.txt\n@@ -1 +1 @@\n-short line\n+{}\n",
                "x".repeat(long_line_bytes)
            ),
        }),
    });
}

fn added_rust_patch(line_count: usize) -> String {
    use std::fmt::Write;

    let mut patch = format!(
        "diff --git a/generated/huge.rs b/generated/huge.rs\nnew file mode 100644\n--- /dev/null\n+++ b/generated/huge.rs\n@@ -0,0 +1,{line_count} @@\n"
    );
    for index in 0..line_count {
        writeln!(patch, "+pub const ITEM_{index:06}: usize = {index};")
            .expect("writing to a String cannot fail");
    }
    patch
}

fn added_json_patch(item_count: usize) -> String {
    use std::fmt::Write;

    let line_count = item_count.saturating_add(2);
    let mut patch = format!(
        "diff --git a/generated/large.json b/generated/large.json\nnew file mode 100644\n--- /dev/null\n+++ b/generated/large.json\n@@ -0,0 +1,{line_count} @@\n+[\n"
    );
    for index in 0..item_count {
        let comma = if index + 1 == item_count { "" } else { "," };
        writeln!(patch, "+  {{\"index\": {index}, \"enabled\": true}}{comma}")
            .expect("writing to a String cannot fail");
    }
    patch.push_str("+]\n");
    patch
}

impl RepositorySource for MutableFixtureRepository {
    fn snapshot(&self) -> Result<RepositorySnapshot> {
        Ok(self.snapshot.borrow().clone())
    }
}

impl Repository for MutableFixtureRepository {
    fn access_mode(&self) -> AccessMode {
        AccessMode::ReadWrite
    }

    fn apply(&self, action: &RepositoryAction) -> Result<()> {
        match action {
            RepositoryAction::Stage(path) => self.stage(path),
            RepositoryAction::Unstage(path) => self.unstage(path),
            RepositoryAction::StageAll => {
                let paths = self
                    .snapshot
                    .borrow()
                    .files
                    .iter()
                    .filter(|file| file.unstaged.is_some())
                    .map(|file| file.path.clone())
                    .collect::<Vec<_>>();
                for path in paths {
                    self.stage(&path)?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{AccessMode, ChangeKind, Repository, RepositoryAction, RepositorySource};

    use super::{FixtureRepositorySource, MutableFixtureRepository};

    #[test]
    fn loads_structured_snapshot() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("repository-state.ron");

        let snapshot = FixtureRepositorySource::new(fixture)
            .snapshot()
            .expect("fixture should load");

        assert_eq!(
            snapshot.branch.name.as_deref(),
            Some("feature/syntax-highlighting")
        );
        assert!(snapshot.files.iter().any(|file| file.staged.is_some()));
        assert!(snapshot.files.iter().any(|file| file.unstaged.is_some()));
        assert!(
            snapshot
                .files
                .iter()
                .any(|file| file.kind == ChangeKind::Untracked)
        );
        assert!(!snapshot.recent_commits.is_empty());
        assert!(
            snapshot
                .files
                .iter()
                .any(|file| file.path == Path::new("web/app.tsx"))
        );
        assert!(
            snapshot
                .files
                .iter()
                .any(|file| file.path == Path::new("scripts/report.py"))
        );
        assert_eq!(snapshot.upstream.expect("upstream should exist").ahead, 2);
    }

    #[test]
    fn mutable_fixture_stages_and_unstages_without_changing_the_file() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("repository-state.ron");
        let repository = MutableFixtureRepository::new(&fixture).expect("fixture should load");
        let path = Path::new("examples/new_tool.rs");

        assert_eq!(repository.access_mode(), AccessMode::ReadWrite);
        repository
            .apply(&RepositoryAction::Stage(path.to_path_buf()))
            .expect("mock stage should work");
        let staged = repository.snapshot().expect("snapshot after stage");
        let file = staged
            .files
            .iter()
            .find(|file| file.path == path)
            .expect("mock file");
        assert!(file.staged.is_some());
        assert!(file.unstaged.is_none());
        assert_eq!(file.kind, ChangeKind::Added);

        repository
            .apply(&RepositoryAction::Unstage(path.to_path_buf()))
            .expect("mock unstage should work");
        let unstaged = repository.snapshot().expect("snapshot after unstage");
        let file = unstaged
            .files
            .iter()
            .find(|file| file.path == path)
            .expect("mock file");
        assert!(file.staged.is_none());
        assert!(file.unstaged.is_some());
        assert_eq!(file.kind, ChangeKind::Untracked);

        let fixture_snapshot = FixtureRepositorySource::new(fixture)
            .snapshot()
            .expect("fixture should still load");
        let file = fixture_snapshot
            .files
            .iter()
            .find(|file| file.path == path)
            .expect("fixture file");
        assert!(file.staged.is_none());
        assert!(file.unstaged.is_some());
    }

    #[test]
    fn generates_large_files_on_demand() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("repository-state.ron");
        let repository = MutableFixtureRepository::new_with_large_files(fixture)
            .expect("large fixture should generate");
        let snapshot = repository.snapshot().expect("large snapshot");

        let rust = snapshot
            .files
            .iter()
            .find(|file| file.path == Path::new("generated/huge.rs"))
            .and_then(|file| file.unstaged.as_ref())
            .expect("generated Rust diff");
        assert_eq!(
            rust.text
                .lines()
                .filter(|line| line.starts_with("+pub const ITEM_"))
                .count(),
            100_000
        );

        let long_line = snapshot
            .files
            .iter()
            .find(|file| file.path == Path::new("generated/long-line.txt"))
            .and_then(|file| file.unstaged.as_ref())
            .expect("generated long-line diff");
        assert!(
            long_line
                .text
                .lines()
                .any(|line| line.starts_with('+') && line.len() == 100_001)
        );
    }
}
