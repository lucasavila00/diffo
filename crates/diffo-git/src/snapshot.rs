use std::{
    collections::BTreeSet,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use diffo_core::{ChangeKind, Commit, FileDiff, FileState, RepositorySnapshot, RepositorySource};

use super::{GitRepositorySource, NO_CHANGE, status::parse_status};

impl GitRepositorySource {
    /// Return the worktree and external Git metadata paths that affect snapshots.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot resolve repository paths.
    pub fn watch_paths(&self) -> Result<Vec<PathBuf>> {
        let mut paths = BTreeSet::new();
        for args in [
            &["rev-parse", "--show-toplevel"][..],
            &["rev-parse", "--path-format=absolute", "--git-dir"][..],
            &["rev-parse", "--path-format=absolute", "--git-common-dir"][..],
        ] {
            let output = String::from_utf8(self.git(args)?)
                .context("git returned a non-UTF-8 repository path")?;
            paths.insert(PathBuf::from(output.trim()));
        }
        Ok(paths.into_iter().collect())
    }

    fn diff(&self, paths: &[&str], staged: bool) -> Result<Option<FileDiff>> {
        // Ask Git for the complete file, not only its usual three lines of
        // context. The diff layer still keeps each change block separate.
        let mut args = vec![
            "diff",
            "--no-ext-diff",
            "--no-color",
            "--unified=2147483647",
        ];
        if staged {
            args.push("--cached");
        }
        args.push("--");
        args.extend(paths);

        let text = String::from_utf8(self.git(&args)?)
            .with_context(|| format!("git returned a non-UTF-8 diff for {}", paths.join(", ")))?;
        Ok((!text.is_empty()).then_some(FileDiff { text }))
    }

    pub(super) fn worktree_file_diff(&self, path: &Path) -> Result<FileDiff> {
        let full_path = self.root.join(path);
        let metadata = fs::symlink_metadata(&full_path)
            .with_context(|| format!("failed to inspect worktree file {}", path.display()))?;
        let bytes = if metadata.file_type().is_symlink() {
            fs::read_link(&full_path)
                .with_context(|| format!("failed to read worktree symlink {}", path.display()))?
                .to_string_lossy()
                .into_owned()
                .into_bytes()
        } else {
            fs::read(&full_path)
                .with_context(|| format!("failed to read worktree file {}", path.display()))?
        };

        let Ok(contents) = std::str::from_utf8(&bytes) else {
            return Ok(FileDiff {
                text: format!("Binary files /dev/null and b/{} differ\n", path.display()),
            });
        };
        if bytes.contains(&0) {
            return Ok(FileDiff {
                text: format!("Binary files /dev/null and b/{} differ\n", path.display()),
            });
        }

        let line_count = contents.lines().count();
        let mut text = format!("@@ -0,0 +1,{line_count} @@\n");
        for line in contents.split_inclusive('\n') {
            text.push('+');
            text.push_str(line);
        }
        if !contents.is_empty() && !contents.ends_with('\n') {
            text.push('\n');
            text.push_str("\\ No newline at end of file\n");
        }
        Ok(FileDiff { text })
    }

    fn rename_context(
        &self,
        diff: Option<FileDiff>,
        path: &Path,
        staged: bool,
    ) -> Result<Option<FileDiff>> {
        let Some(mut diff) = diff else {
            return Ok(None);
        };
        if diff.text.lines().any(|line| line.starts_with("@@ ")) {
            return Ok(Some(diff));
        }

        let bytes = if staged {
            let spec = format!(":{}", path.to_string_lossy());
            self.git(&["show", &spec])?
        } else {
            fs::read(self.root.join(path))
                .with_context(|| format!("failed to read renamed file {}", path.display()))?
        };
        append_context_hunk(&mut diff.text, path, &bytes);
        Ok(Some(diff))
    }

    fn recent_commits(&self) -> Result<Vec<Commit>> {
        let has_head = Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&self.root)
            .output()
            .context("failed to check Git HEAD")?
            .status
            .success();
        if !has_head {
            return Ok(Vec::new());
        }

        let output = String::from_utf8(self.git(&["log", "-n", "50", "--format=%H%x00%s%x00"])?)
            .context("git returned a non-UTF-8 commit log")?;
        let fields = output.split('\0').collect::<Vec<_>>();

        Ok(fields
            .chunks(2)
            .filter_map(|fields| match fields {
                [id, summary] if !id.trim().is_empty() => Some(Commit {
                    id: id.trim().to_owned(),
                    summary: (*summary).to_owned(),
                }),
                _ => None,
            })
            .collect())
    }
}

impl RepositorySource for GitRepositorySource {
    fn snapshot(&self) -> Result<RepositorySnapshot> {
        let status = self.git(&[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
            "-z",
        ])?;
        let parsed = parse_status(&status)?;
        let mut files = Vec::with_capacity(parsed.files.len());

        for file in parsed.files {
            let path = file.state.path.to_string_lossy();
            let old_path = file
                .state
                .old_path
                .as_ref()
                .map(|path| path.to_string_lossy());
            let paths = old_path
                .as_deref()
                .map_or_else(|| vec![path.as_ref()], |old| vec![old, path.as_ref()]);
            let conflicted = file.state.kind == ChangeKind::Conflicted;
            let mut staged = if conflicted || file.index_status == NO_CHANGE {
                None
            } else {
                self.diff(&paths, true)?
            };
            let mut unstaged = if matches!(
                file.state.kind,
                ChangeKind::Untracked | ChangeKind::Conflicted
            ) {
                Some(self.worktree_file_diff(&file.state.path)?)
            } else if file.worktree_status == NO_CHANGE {
                None
            } else {
                self.diff(&paths, false)?
            };
            if matches!(file.state.kind, ChangeKind::Renamed | ChangeKind::Copied) {
                staged = self.rename_context(staged, &file.state.path, true)?;
                unstaged = self.rename_context(unstaged, &file.state.path, false)?;
            }
            files.push(FileState {
                staged,
                unstaged,
                ..file.state
            });
        }

        Ok(RepositorySnapshot {
            branch: parsed.branch,
            files,
            recent_commits: self.recent_commits()?,
            upstream: parsed.upstream,
        })
    }
}

fn append_context_hunk(output: &mut String, path: &Path, bytes: &[u8]) {
    let Ok(contents) = std::str::from_utf8(bytes) else {
        writeln!(
            output,
            "Binary files a/{} and b/{} differ",
            path.display(),
            path.display()
        )
        .expect("writing to a String cannot fail");
        return;
    };
    if bytes.contains(&0) {
        writeln!(
            output,
            "Binary files a/{} and b/{} differ",
            path.display(),
            path.display()
        )
        .expect("writing to a String cannot fail");
        return;
    }

    let line_count = contents.lines().count();
    writeln!(
        output,
        "@@ -1,{line_count} +1,{line_count} @@ Renamed file contents"
    )
    .expect("writing to a String cannot fail");
    for line in contents.split_inclusive('\n') {
        output.push(' ');
        output.push_str(line);
    }
    if !contents.is_empty() && !contents.ends_with('\n') {
        output.push('\n');
        output.push_str("\\ No newline at end of file\n");
    }
}
