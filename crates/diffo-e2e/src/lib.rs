#![doc = include_str!("../README.md")]

mod git_proxy;
mod input;
mod reader;
mod screen;
mod selectors;
mod types;

pub use git_proxy::{GitGatePhase, GitProxy};
pub use screen::DiffoScreen;
pub use types::{Key, ScrollDirection, Selector};

/// Resolves the binary every black-box test must launch.
///
/// `DIFFO_E2E_BINARY` is a developer and release-test hook. When it is absent,
/// callers retain their normal Cargo-provided or locally built test binary.
///
/// # Errors
///
/// Returns an error when the override is not an absolute regular-file path.
pub fn diffo_binary(default: impl AsRef<std::path::Path>) -> anyhow::Result<std::path::PathBuf> {
    let Some(override_path) = std::env::var_os("DIFFO_E2E_BINARY") else {
        return Ok(default.as_ref().to_owned());
    };
    let path = std::path::PathBuf::from(override_path);
    anyhow::ensure!(path.is_absolute(), "DIFFO_E2E_BINARY must be absolute");
    let path = std::fs::canonicalize(&path).map_err(|error| {
        anyhow::anyhow!(
            "could not resolve DIFFO_E2E_BINARY {}: {error}",
            path.display()
        )
    })?;
    anyhow::ensure!(
        path.is_file(),
        "DIFFO_E2E_BINARY is not a regular file: {}",
        path.display()
    );
    Ok(path)
}
