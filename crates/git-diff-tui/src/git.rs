use std::{env, fs, path::Path, process::Command};

use anyhow::{Context, Result, bail};

pub fn working_tree_diff() -> Result<String> {
    if let Some(path) = env::var_os("DIFFO_MOCK_FILE") {
        return mock_diff(Path::new(&path));
    }

    let output = Command::new("git")
        .args(["diff", "--no-ext-diff", "--color=never"])
        .output()
        .context("failed to run git; is it installed and available on PATH?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git diff failed: {}", stderr.trim());
    }

    let diff = String::from_utf8(output.stdout).context("git returned a non-UTF-8 diff")?;
    Ok(if diff.is_empty() {
        "No unstaged changes.".to_owned()
    } else {
        diff
    })
}

fn mock_diff(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read mock repository state from {}",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::mock_diff;

    #[test]
    fn loads_mock_diff_from_a_file() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("repository-state.diff");

        let diff = mock_diff(&fixture).expect("fixture should load");

        assert!(diff.contains("STAGED CHANGES"));
        assert!(diff.contains("UNTRACKED FILES"));
        assert!(diff.contains("RECENT COMMITS"));
        assert!(diff.contains("PUSH STATUS"));
    }
}
