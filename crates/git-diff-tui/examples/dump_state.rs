use std::env;

use anyhow::{Context, Result};
use git_diff_tui::{git_source::GitRepositorySource, repository::RepositorySource};
use ron::ser::PrettyConfig;

fn main() -> Result<()> {
    let root = env::var_os("DIFFO_REPOSITORY").unwrap_or_else(|| ".".into());
    let snapshot = GitRepositorySource::new(root).snapshot()?;
    let dump = ron::ser::to_string_pretty(&snapshot, PrettyConfig::default())
        .context("failed to serialize repository snapshot")?;
    println!("{dump}");
    Ok(())
}
