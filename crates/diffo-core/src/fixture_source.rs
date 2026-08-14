use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use anyhow::{Context, Result, bail};

use crate::{
    FailureKind, OperationFailure, OperationResult, Repository, RepositoryAction,
    RepositorySnapshot, RepositorySource,
};

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

pub struct MutableFixtureRepository {
    snapshot: Mutex<RepositorySnapshot>,
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

    /// Load a mutable fixture and append generated large-file and file-list stress changes.
    ///
    /// # Errors
    ///
    /// Returns an error when the base fixture cannot be read or parsed.
    pub fn new_with_large_files(path: impl Into<PathBuf>) -> Result<Self> {
        let mut snapshot = FixtureRepositorySource::new(path).snapshot()?;
        append_large_files(&mut snapshot, 20_000, 5_000, 25_000);
        append_line_stress_files(&mut snapshot);
        append_file_list_stress_files(&mut snapshot, 250);
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
            snapshot: Mutex::new(snapshot),
            untracked_paths,
        }
    }

    fn stage(&self, path: &Path) -> Result<()> {
        let mut snapshot = self.snapshot.lock().expect("mock snapshot mutex poisoned");
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
        let mut snapshot = self.snapshot.lock().expect("mock snapshot mutex poisoned");
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

    fn commit(&self, message: &str) -> OperationResult {
        let mut snapshot = self.snapshot.lock().expect("mock snapshot mutex poisoned");
        snapshot.files.retain_mut(|file| {
            file.staged = None;
            file.unstaged.is_some()
        });
        snapshot.recent_commits.insert(
            0,
            crate::Commit {
                id: "mock-commit".to_owned(),
                summary: message.to_owned(),
            },
        );
        if let Some(upstream) = snapshot.upstream.as_mut() {
            upstream.ahead = upstream.ahead.saturating_add(1);
            upstream.recent_local_commits.insert(
                0,
                crate::Commit {
                    id: "mock-commit".to_owned(),
                    summary: message.to_owned(),
                },
            );
            upstream.recent_local_commits.truncate(3);
        }
        OperationResult::Commit {
            hash: "mock-commit".to_owned(),
        }
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

fn append_line_stress_files(snapshot: &mut RepositorySnapshot) {
    for (name, line_count) in [
        ("stress-5k.rs", 5_000),
        ("stress-50k.rs", 50_000),
        ("stress-500k.rs", 500_000),
        ("stress-5000k.rs", 5_000_000),
    ] {
        snapshot.files.push(crate::FileState {
            path: PathBuf::from("generated").join(name),
            old_path: None,
            kind: crate::ChangeKind::Untracked,
            staged: None,
            unstaged: Some(crate::FileDiff {
                text: added_source_stress_patch(name, line_count),
            }),
        });
    }
}

fn append_file_list_stress_files(snapshot: &mut RepositorySnapshot, file_count: usize) {
    for index in 0..file_count {
        let path = format!("generated/file-list/file-{index:03}.rs");
        snapshot.files.push(crate::FileState {
            path: PathBuf::from(&path),
            old_path: None,
            kind: crate::ChangeKind::Untracked,
            staged: None,
            unstaged: Some(crate::FileDiff {
                text: format!(
                    "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1 @@\n+pub const FILE_INDEX: usize = {index};\n"
                ),
            }),
        });
    }
}

fn added_source_stress_patch(name: &str, line_count: usize) -> String {
    use std::fmt::Write;

    let mut patch = String::with_capacity(line_count.saturating_mul(28).saturating_add(160));
    write!(
        patch,
        "diff --git a/generated/{name} b/generated/{name}\nnew file mode 100644\n--- /dev/null\n+++ b/generated/{name}\n@@ -0,0 +1,{line_count} @@\n"
    )
    .expect("writing to a String cannot fail");
    let mut random = 0x9e37_79b9_u32;
    for _ in 0..line_count {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        match random & 3 {
            0 => writeln!(patch, "+const FLAG_{random:08X}: bool = true;"),
            1 => writeln!(patch, "+pub const LIMIT_{random:08X}: u32 = {random};"),
            2 => writeln!(patch, "+static NAME_{random:08X}: &str = \"worker\";"),
            _ => writeln!(patch, "+fn task_{random:08x}() -> u32 {{ {random} }}"),
        }
        .expect("writing to a String cannot fail");
    }
    patch
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
        Ok(self
            .snapshot
            .lock()
            .expect("mock snapshot mutex poisoned")
            .clone())
    }
}

impl Repository for MutableFixtureRepository {
    fn commit_patch(&self, commit_id: &str) -> Result<String> {
        let snapshot = self.snapshot.lock().expect("mock snapshot mutex poisoned");
        if !snapshot
            .recent_commits
            .iter()
            .any(|commit| commit.id == commit_id)
        {
            bail!("mock repository has no commit {commit_id}");
        }
        Ok(snapshot
            .files
            .iter()
            .filter_map(|file| file.staged.as_ref().or(file.unstaged.as_ref()))
            .take(2)
            .map(|diff| diff.text.as_str())
            .collect::<Vec<_>>()
            .join(""))
    }

    fn apply(
        &self,
        action: &RepositoryAction,
    ) -> std::result::Result<OperationResult, OperationFailure> {
        if let RepositoryAction::GuardedCommit(target) = action {
            let snapshot = self.snapshot.lock().expect("mock snapshot mutex poisoned");
            if snapshot.head != target.expected_head
                || snapshot.staged_files() != target.expected_staged
            {
                return Err(OperationFailure {
                    action: action.clone(),
                    kind: FailureKind::RefChanged,
                    detail: "staged changes changed; press i to generate a new commit message"
                        .to_owned(),
                });
            }
        }
        let result = (|| -> Result<OperationResult> {
            match action {
                RepositoryAction::Stage(path) => self.stage(path).map(|()| OperationResult::Stage),
                RepositoryAction::Unstage(path) => {
                    self.unstage(path).map(|()| OperationResult::Unstage)
                }
                RepositoryAction::StageAll => {
                    let paths = self
                        .snapshot
                        .lock()
                        .expect("mock snapshot mutex poisoned")
                        .files
                        .iter()
                        .filter(|file| file.unstaged.is_some())
                        .map(|file| file.path.clone())
                        .collect::<Vec<_>>();
                    for path in paths {
                        self.stage(&path)?;
                    }
                    Ok(OperationResult::Stage)
                }
                RepositoryAction::UnstageAll => {
                    let paths = self
                        .snapshot
                        .lock()
                        .expect("mock snapshot mutex poisoned")
                        .files
                        .iter()
                        .filter(|file| file.staged.is_some())
                        .map(|file| file.path.clone())
                        .collect::<Vec<_>>();
                    for path in paths {
                        self.unstage(&path)?;
                    }
                    Ok(OperationResult::Unstage)
                }
                RepositoryAction::Commit(message) => Ok(self.commit(message)),
                RepositoryAction::GuardedCommit(target) => Ok(self.commit(&target.message)),
                RepositoryAction::Fetch
                | RepositoryAction::Sync
                | RepositoryAction::SyncToRemote(_)
                | RepositoryAction::Checkout(_)
                | RepositoryAction::CreateBranch(_)
                | RepositoryAction::DeleteBranch(_)
                | RepositoryAction::Merge(_)
                | RepositoryAction::AbortMerge
                | RepositoryAction::Discard(_)
                | RepositoryAction::DiscardAll(_)
                | RepositoryAction::Stash { .. }
                | RepositoryAction::ApplyStash(_)
                | RepositoryAction::DropStash(_)
                | RepositoryAction::Amend(_)
                | RepositoryAction::UndoLastCommit(_)
                | RepositoryAction::Revert(_)
                | RepositoryAction::RenameBranch(_) => {
                    bail!("mock repository cannot execute {action:?}: no remote configured")
                }
            }
        })();
        result.map_err(|error| OperationFailure {
            action: action.clone(),
            kind: if matches!(
                action,
                RepositoryAction::Fetch
                    | RepositoryAction::Sync
                    | RepositoryAction::SyncToRemote(_)
            ) {
                FailureKind::NoRemote
            } else {
                FailureKind::Unknown
            },
            detail: error.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, path::Path};

    use crate::{
        ChangeKind, GuardedCommitTarget, HeadState, OperationResult, Repository, RepositoryAction,
        RepositorySource,
    };

    use super::{FixtureRepositorySource, MutableFixtureRepository};

    #[test]
    fn mutable_fixture_stages_and_unstages_without_changing_the_file() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("repository-state.ron");
        let repository = MutableFixtureRepository::new(&fixture).expect("fixture should load");
        let path = Path::new("examples/new_tool.rs");

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
    fn mock_remote_error_names_the_executed_action() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("repository-state.ron");
        let repository = MutableFixtureRepository::new(fixture).expect("fixture should load");

        for action in [RepositoryAction::Fetch, RepositoryAction::Sync] {
            let action_name = format!("{action:?}");
            let error = repository
                .apply(&action)
                .expect_err("mock remote action should fail");
            let message = error.to_string();
            assert!(message.contains(&action_name), "{message}");
            assert!(message.contains("no remote configured"), "{message}");
        }
    }

    #[test]
    fn mutable_fixture_enforces_guarded_commit_state() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("repository-state.ron");
        let repository = MutableFixtureRepository::new(&fixture).expect("fixture should load");
        let path = Path::new("examples/new_tool.rs");
        repository
            .apply(&RepositoryAction::Stage(path.to_path_buf()))
            .expect("mock stage should work");
        let snapshot = repository.snapshot().expect("snapshot after stage");
        let target = GuardedCommitTarget {
            message: "feat: fixture commit".to_owned(),
            expected_head: snapshot.head.clone(),
            expected_staged: snapshot.staged_files(),
        };

        assert!(matches!(
            repository.apply(&RepositoryAction::GuardedCommit(Box::new(target))),
            Ok(OperationResult::Commit { .. })
        ));

        let stale = GuardedCommitTarget {
            message: "must not commit".to_owned(),
            expected_head: HeadState::Unborn {
                name: "stale".to_owned(),
            },
            expected_staged: Vec::new(),
        };
        assert!(
            repository
                .apply(&RepositoryAction::GuardedCommit(Box::new(stale)))
                .is_err()
        );
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
            20_000
        );
        assert_eq!(
            snapshot
                .files
                .iter()
                .filter(|file| {
                    file.path.starts_with("generated/file-list") && file.unstaged.is_some()
                })
                .count(),
            250
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
                .any(|line| line.starts_with('+') && line.len() == 25_001)
        );

        for (name, expected_lines) in [
            ("stress-5k.rs", 5_000),
            ("stress-50k.rs", 50_000),
            ("stress-500k.rs", 500_000),
            ("stress-5000k.rs", 5_000_000),
        ] {
            let diff = snapshot
                .files
                .iter()
                .find(|file| file.path == Path::new("generated").join(name))
                .and_then(|file| file.unstaged.as_ref())
                .expect("generated line stress diff");
            let hunk = diff.text.find("@@\n").expect("stress diff hunk");
            assert_eq!(
                diff.text[hunk + 3..]
                    .lines()
                    .filter(|line| line.starts_with('+'))
                    .count(),
                expected_lines
            );
            assert!(diff.text.contains("+const FLAG_"));
            assert!(diff.text.contains("+fn task_"));
            let sample = diff.text.lines().skip(6).take(1_000).collect::<Vec<_>>();
            let unique = sample.iter().collect::<HashSet<_>>();
            assert_eq!(unique.len(), sample.len());
        }
    }
}
