use std::{fs, path::PathBuf};

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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{ChangeKind, RepositorySource};

    use super::FixtureRepositorySource;

    #[test]
    fn loads_structured_snapshot() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("repository-state.ron");

        let snapshot = FixtureRepositorySource::new(fixture)
            .snapshot()
            .expect("fixture should load");

        assert_eq!(snapshot.branch.name.as_deref(), Some("feature/mock-state"));
        assert!(snapshot.files.iter().any(|file| file.staged.is_some()));
        assert!(snapshot.files.iter().any(|file| file.unstaged.is_some()));
        assert!(
            snapshot
                .files
                .iter()
                .any(|file| file.kind == ChangeKind::Untracked)
        );
        assert!(!snapshot.recent_commits.is_empty());
        assert_eq!(snapshot.upstream.expect("upstream should exist").ahead, 2);
    }
}
