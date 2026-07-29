use std::{
    collections::BTreeSet,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
};

use anyhow::{Context, Result, anyhow};
use diffo_core::{
    ChangeKind, Commit, FileDiff, FileState, HeadState, RepositorySnapshot, RepositorySource,
    RepositoryWatchPaths,
};

use super::{
    GitRepositorySource, NO_CHANGE,
    status::{ParsedFile, parse_status},
};

const MAX_SNAPSHOT_WORKERS: usize = 8;

impl GitRepositorySource {
    /// Return the worktree and external Git metadata paths that affect snapshots.
    ///
    /// # Errors
    ///
    /// Returns an error when Git cannot resolve repository paths.
    pub fn watch_paths(&self) -> Result<RepositoryWatchPaths> {
        let worktree = self.repository_root()?;
        let mut git_metadata = BTreeSet::new();
        for args in [
            &["rev-parse", "--path-format=absolute", "--git-dir"][..],
            &["rev-parse", "--path-format=absolute", "--git-common-dir"][..],
        ] {
            let output = String::from_utf8(self.git(args)?)
                .context("git returned a non-UTF-8 repository path")?;
            git_metadata.insert(PathBuf::from(output.trim()));
        }
        Ok(RepositoryWatchPaths {
            worktree,
            git_metadata: git_metadata.into_iter().collect(),
        })
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

    fn recent_local_commits(&self, upstream: &str) -> Result<Vec<Commit>> {
        let output = String::from_utf8(self.git(&[
            "log",
            "-n",
            "3",
            "--format=%H%x00%s%x00",
            "HEAD",
            "--not",
            upstream,
        ])?)
        .context("git returned a non-UTF-8 local commit log")?;
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

    fn file_state(&self, file: ParsedFile) -> Result<Option<FileState>> {
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
        let mut unstaged = if file.state.kind == ChangeKind::Untracked {
            match self.worktree_file_diff(&file.state.path) {
                Ok(diff) => Some(diff),
                // Git status and worktree reads are not atomic. Generators can
                // remove an untracked temporary file between those observations.
                Err(error) if error_is_not_found(&error) => return Ok(None),
                Err(error) => return Err(error),
            }
        } else if conflicted {
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
        Ok(Some(FileState {
            staged,
            unstaged,
            ..file.state
        }))
    }

    fn file_states(&self, files: Vec<ParsedFile>) -> Result<Vec<FileState>> {
        let worker_count = files.len().min(MAX_SNAPSHOT_WORKERS);
        if worker_count <= 1 {
            let files = files
                .into_iter()
                .map(|file| self.file_state(file))
                .collect::<Result<Vec<_>>>()?;
            return Ok(files.into_iter().flatten().collect());
        }

        let mut buckets = (0..worker_count).map(|_| Vec::new()).collect::<Vec<_>>();
        for (index, file) in files.into_iter().enumerate() {
            buckets[index % worker_count].push((index, file));
        }
        let mut completed = thread::scope(|scope| {
            let workers = buckets
                .into_iter()
                .map(|bucket| {
                    scope.spawn(move || {
                        bucket
                            .into_iter()
                            .map(|(index, file)| self.file_state(file).map(|file| (index, file)))
                            .collect::<Result<Vec<_>>>()
                    })
                })
                .collect::<Vec<_>>();
            let outcomes = workers
                .into_iter()
                .map(std::thread::ScopedJoinHandle::join)
                .collect::<Vec<_>>();
            let mut completed = Vec::new();
            for outcome in outcomes {
                completed
                    .extend(outcome.map_err(|_| anyhow!("repository snapshot worker panicked"))??);
            }
            Ok::<_, anyhow::Error>(completed)
        })?;
        completed.sort_unstable_by_key(|(index, _)| *index);
        Ok(completed.into_iter().filter_map(|(_, file)| file).collect())
    }
}

impl RepositorySource for GitRepositorySource {
    fn snapshot(&self) -> Result<RepositorySnapshot> {
        let status = self.git(&[
            "--no-optional-locks",
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
            "-z",
        ])?;
        let mut parsed = parse_status(&status)?;
        let files = self.file_states(parsed.files)?;
        if matches!(parsed.head, HeadState::Named { .. })
            && let Some(upstream) = &mut parsed.upstream
        {
            upstream.recent_local_commits = self.recent_local_commits(&upstream.name)?;
        }

        Ok(RepositorySnapshot {
            head: parsed.head,
            files,
            recent_commits: self.recent_commits()?,
            upstream: parsed.upstream,
        })
    }
}

fn error_is_not_found(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed_file(path: &str, kind: ChangeKind) -> ParsedFile {
        ParsedFile {
            state: FileState {
                path: PathBuf::from(path),
                old_path: None,
                kind,
                staged: None,
                unstaged: None,
            },
            index_status: NO_CHANGE,
            worktree_status: NO_CHANGE,
        }
    }

    #[test]
    fn omits_a_vanished_untracked_file() {
        let root = tempfile::tempdir().expect("repository directory");
        let source = GitRepositorySource::new(root.path());

        let files = source
            .file_states(vec![parsed_file("vanished.txt", ChangeKind::Untracked)])
            .expect("collect file states");

        assert!(files.is_empty());
    }

    #[test]
    fn omits_vanished_untracked_files_without_losing_surviving_files() {
        let root = tempfile::tempdir().expect("repository directory");
        fs::write(root.path().join("surviving.txt"), "surviving\n").expect("write surviving file");
        let source = GitRepositorySource::new(root.path());

        let files = source
            .file_states(vec![
                parsed_file("vanished.txt", ChangeKind::Untracked),
                parsed_file("surviving.txt", ChangeKind::Untracked),
            ])
            .expect("collect file states");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, Path::new("surviving.txt"));
        assert_eq!(
            files[0].unstaged.as_ref().map(|diff| diff.text.as_str()),
            Some("@@ -0,0 +1,1 @@\n+surviving\n")
        );
    }

    #[test]
    fn missing_conflicted_files_remain_errors() {
        let root = tempfile::tempdir().expect("repository directory");
        let source = GitRepositorySource::new(root.path());

        let error = source
            .file_states(vec![parsed_file("missing.txt", ChangeKind::Conflicted)])
            .expect_err("missing conflict must fail");

        assert!(
            error
                .to_string()
                .contains("failed to inspect worktree file missing.txt")
        );
    }

    #[test]
    fn non_not_found_untracked_file_errors_remain_errors() {
        let root = tempfile::tempdir().expect("repository directory");
        fs::create_dir(root.path().join("directory")).expect("create directory");
        let source = GitRepositorySource::new(root.path());

        let error = source
            .file_states(vec![parsed_file("directory", ChangeKind::Untracked)])
            .expect_err("reading a directory as a file must fail");

        assert!(
            error
                .to_string()
                .contains("failed to read worktree file directory")
        );
    }
}
