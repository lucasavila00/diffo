use std::{
    collections::BTreeSet,
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail};

use diffo_core::{
    AccessMode, BranchState, ChangeKind, Commit, FailureKind, FileDiff, FileState,
    OperationFailure, OperationResult, Repository, RepositoryAction, RepositorySnapshot,
    RepositorySource, UpstreamState,
};

const NO_CHANGE: char = '.';

pub struct GitRepositorySource {
    root: PathBuf,
    access_mode: AccessMode,
}

impl GitRepositorySource {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            access_mode: AccessMode::ReadWrite,
        }
    }

    #[must_use]
    pub fn with_access_mode(mut self, access_mode: AccessMode) -> Self {
        self.access_mode = access_mode;
        self
    }

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

    fn git(&self, args: &[&str]) -> Result<Vec<u8>> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()
            .context("failed to run git; is it installed and available on PATH?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git {} failed: {}", args.join(" "), stderr.trim());
        }

        Ok(output.stdout)
    }

    fn diff(&self, paths: &[&str], staged: bool) -> Result<Option<FileDiff>> {
        let mut args = vec!["diff", "--no-ext-diff", "--no-color"];
        if staged {
            args.push("--cached");
        }
        args.push("--");
        args.extend(paths);

        let text = String::from_utf8(self.git(&args)?)
            .with_context(|| format!("git returned a non-UTF-8 diff for {}", paths.join(", ")))?;
        Ok((!text.is_empty()).then_some(FileDiff { text }))
    }

    fn worktree_file_diff(&self, path: &Path) -> Result<FileDiff> {
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

impl Default for GitRepositorySource {
    fn default() -> Self {
        Self::new(".")
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

impl Repository for GitRepositorySource {
    fn access_mode(&self) -> AccessMode {
        self.access_mode
    }

    fn apply(
        &self,
        action: &RepositoryAction,
    ) -> std::result::Result<OperationResult, OperationFailure> {
        if self.access_mode == AccessMode::ReadOnly {
            return Err(operation_failure(
                action,
                FailureKind::Unknown,
                "repository is read-only",
            ));
        }

        if matches!(
            action,
            RepositoryAction::Fetch | RepositoryAction::Pull | RepositoryAction::Push
        ) && let Some(delay) = e2e_network_delay()
        {
            thread::sleep(delay);
        }

        let before_head = matches!(action, RepositoryAction::Pull)
            .then(|| self.git(&["rev-parse", "HEAD"]))
            .transpose()
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?
            .map(|head| String::from_utf8_lossy(&head).trim().to_owned());
        let before_fetch = matches!(action, RepositoryAction::Fetch)
            .then(|| self.snapshot().ok().and_then(|snapshot| snapshot.upstream))
            .flatten();
        if matches!(action, RepositoryAction::Push)
            && self
                .snapshot()
                .ok()
                .and_then(|snapshot| snapshot.upstream)
                .is_some_and(|upstream| upstream.behind > 0)
        {
            return Err(operation_failure(
                action,
                FailureKind::PullRequired,
                "pull required before push",
            ));
        }

        let mut command = Command::new("git");
        command
            .current_dir(&self.root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true");
        match action {
            RepositoryAction::Stage(path) => {
                command.args(["add", "--"]).arg(path);
            }
            RepositoryAction::Unstage(path) => {
                command.args(["reset", "--"]).arg(path);
            }
            RepositoryAction::StageAll => {
                command.args(["add", "--all"]);
            }
            RepositoryAction::UnstageAll => {
                command.arg("reset");
            }
            RepositoryAction::Fetch => {
                command.arg("fetch");
            }
            RepositoryAction::Pull => {
                command.args(["pull", "--no-edit"]);
            }
            RepositoryAction::Push => {
                command.args(["push", "--porcelain"]);
            }
            RepositoryAction::Commit(message) => {
                command.args(["commit", "-m", message]);
            }
        }

        let output = command.output().map_err(|error| {
            operation_failure(action, FailureKind::Unknown, &error.to_string())
        })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(classify_failure(action, &format!("{stdout}\n{stderr}")));
        }
        match action {
            RepositoryAction::Stage(_) | RepositoryAction::StageAll => Ok(OperationResult::Stage),
            RepositoryAction::Unstage(_) | RepositoryAction::UnstageAll => {
                Ok(OperationResult::Unstage)
            }
            RepositoryAction::Fetch => {
                let after = self.snapshot().ok().and_then(|snapshot| snapshot.upstream);
                let updated_refs = usize::from(before_fetch != after);
                Ok(OperationResult::Fetch { updated_refs })
            }
            RepositoryAction::Pull => {
                let old = before_head.unwrap_or_default();
                let new = self
                    .git(&["rev-parse", "HEAD"])
                    .map_err(|error| {
                        operation_failure(action, FailureKind::Unknown, &error.to_string())
                    })
                    .map(|head| String::from_utf8_lossy(&head).trim().to_owned())?;
                let range = format!("{old}..{new}");
                let commits = self
                    .git(&["rev-list", "--count", &range])
                    .ok()
                    .and_then(|count| String::from_utf8(count).ok())
                    .and_then(|count| count.trim().parse().ok())
                    .unwrap_or(0);
                Ok(OperationResult::Pull { commits })
            }
            RepositoryAction::Push => {
                let snapshot = self.snapshot().map_err(|error| {
                    operation_failure(action, FailureKind::Unknown, &error.to_string())
                })?;
                let hash = snapshot
                    .recent_commits
                    .first()
                    .map_or_else(|| "unknown".to_owned(), |commit| commit.id.clone());
                let upstream = snapshot
                    .upstream
                    .map_or_else(|| "upstream".to_owned(), |upstream| upstream.name);
                Ok(OperationResult::Push { hash, upstream })
            }
            RepositoryAction::Commit(_) => {
                let hash = self
                    .git(&["rev-parse", "HEAD"])
                    .map_err(|error| {
                        operation_failure(action, FailureKind::Unknown, &error.to_string())
                    })
                    .map(|head| String::from_utf8_lossy(&head).trim().to_owned())?;
                Ok(OperationResult::Commit { hash })
            }
        }
    }
}

fn operation_failure(
    action: &RepositoryAction,
    kind: FailureKind,
    detail: &str,
) -> OperationFailure {
    OperationFailure {
        action: action.clone(),
        kind,
        detail: detail.to_owned(),
    }
}

fn classify_failure(action: &RepositoryAction, output: &str) -> OperationFailure {
    let text = output.to_ascii_lowercase();
    let (kind, detail) = if text.contains("non-fast-forward")
        || text.contains("fetch first")
        || text.contains("remote contains work")
    {
        (FailureKind::PushRejected, "remote changed; pull required")
    } else if text.contains("hook declined") || text.contains("pre-receive hook") {
        (FailureKind::HookRejected, "rejected by remote hook")
    } else if text.contains("authentication")
        || text.contains("permission denied")
        || text.contains("could not read username")
    {
        (FailureKind::Authentication, "authentication required")
    } else if text.contains("conflict") {
        (FailureKind::MergeConflict, "resolve repository conflicts")
    } else if text.contains("no configured push destination")
        || text.contains("does not appear to be a git repository")
        || text.contains("no such remote")
    {
        (FailureKind::NoRemote, "no remote configured")
    } else if text.contains("could not resolve host")
        || text.contains("connection refused")
        || text.contains("unable to access")
    {
        (FailureKind::Network, "network unavailable")
    } else if text.contains("local changes") || text.contains("would be overwritten") {
        (FailureKind::DirtyWorktree, "local changes block the operation")
    } else {
        (FailureKind::Unknown, "Git operation failed")
    };
    operation_failure(action, kind, detail)
}

fn e2e_network_delay() -> Option<Duration> {
    let milliseconds = env::var("DIFFO_E2E_NETWORK_DELAY_MS")
        .ok()?
        .parse::<u64>()
        .ok()?
        .min(2_000);
    Some(Duration::from_millis(milliseconds))
}

struct ParsedStatus {
    branch: BranchState,
    files: Vec<ParsedFile>,
    upstream: Option<UpstreamState>,
}

struct ParsedFile {
    state: FileState,
    index_status: char,
    worktree_status: char,
}

fn parse_status(output: &[u8]) -> Result<ParsedStatus> {
    let records = output.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut branch = BranchState::default();
    let mut upstream_name = None;
    let mut ahead = 0;
    let mut behind = 0;
    let mut files = Vec::new();
    let mut index = 0;

    while index < records.len() {
        let record = std::str::from_utf8(records[index]).context("git status was not UTF-8")?;
        index += 1;
        if record.is_empty() {
            continue;
        }

        if let Some(name) = record.strip_prefix("# branch.head ") {
            branch.name = (name != "(detached)").then(|| name.to_owned());
        } else if let Some(name) = record.strip_prefix("# branch.upstream ") {
            upstream_name = Some(name.to_owned());
        } else if let Some(counts) = record.strip_prefix("# branch.ab ") {
            let mut parts = counts.split_whitespace();
            ahead = parse_count(parts.next(), '+')?;
            behind = parse_count(parts.next(), '-')?;
        } else if record.starts_with("1 ") {
            let fields = record.splitn(9, ' ').collect::<Vec<_>>();
            let [_, xy, _, _, _, _, _, _, path] = fields.as_slice() else {
                bail!("invalid ordinary git status record: {record}");
            };
            files.push(parsed_file(path, None, xy)?);
        } else if record.starts_with("2 ") {
            let fields = record.splitn(10, ' ').collect::<Vec<_>>();
            let [_, xy, _, _, _, _, _, _, _, path] = fields.as_slice() else {
                bail!("invalid renamed git status record: {record}");
            };
            let old_path = records
                .get(index)
                .context("rename record has no old path")?;
            index += 1;
            let old_path = std::str::from_utf8(old_path).context("old path was not UTF-8")?;
            files.push(parsed_file(path, Some(old_path), xy)?);
        } else if record.starts_with("u ") {
            let fields = record.splitn(11, ' ').collect::<Vec<_>>();
            let path = fields.last().context("conflict record has no path")?;
            files.push(ParsedFile {
                state: empty_file(path, None, ChangeKind::Conflicted),
                index_status: 'U',
                worktree_status: 'U',
            });
        } else if let Some(path) = record.strip_prefix("? ") {
            files.push(ParsedFile {
                state: empty_file(path, None, ChangeKind::Untracked),
                index_status: NO_CHANGE,
                worktree_status: NO_CHANGE,
            });
        }
    }

    let upstream = upstream_name.map(|name| UpstreamState {
        name,
        ahead,
        behind,
    });
    Ok(ParsedStatus {
        branch,
        files,
        upstream,
    })
}

fn parse_count(value: Option<&str>, prefix: char) -> Result<usize> {
    let value = value.context("branch count is missing")?;
    value
        .strip_prefix(prefix)
        .context("branch count has an invalid prefix")?
        .parse()
        .context("branch count is not a number")
}

fn parsed_file(path: &str, old_path: Option<&str>, xy: &str) -> Result<ParsedFile> {
    let mut statuses = xy.chars();
    let index_status = statuses.next().context("index status is missing")?;
    let worktree_status = statuses.next().context("worktree status is missing")?;
    let status = if index_status == NO_CHANGE {
        worktree_status
    } else {
        index_status
    };
    Ok(ParsedFile {
        state: empty_file(path, old_path, change_kind(status)?),
        index_status,
        worktree_status,
    })
}

fn empty_file(path: &str, old_path: Option<&str>, kind: ChangeKind) -> FileState {
    FileState {
        path: PathBuf::from(path),
        old_path: old_path.map(PathBuf::from),
        kind,
        staged: None,
        unstaged: None,
    }
}

fn change_kind(status: char) -> Result<ChangeKind> {
    match status {
        'A' => Ok(ChangeKind::Added),
        'M' | 'T' => Ok(ChangeKind::Modified),
        'D' => Ok(ChangeKind::Deleted),
        'R' => Ok(ChangeKind::Renamed),
        'C' => Ok(ChangeKind::Copied),
        'U' => Ok(ChangeKind::Conflicted),
        _ => bail!("unknown git file status: {status}"),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
    };

    use super::parse_status;
    use diffo_core::{AccessMode, ChangeKind, Repository, RepositoryAction, RepositorySource};

    #[test]
    fn parses_branch_files_and_upstream() {
        let status = b"# branch.head feature\0# branch.upstream origin/feature\0# branch.ab +2 -1\x001 M. N... 100644 100644 100644 abc def file.txt\0? notes.txt\0";

        let parsed = parse_status(status).expect("status should parse");

        assert_eq!(parsed.branch.name.as_deref(), Some("feature"));
        assert_eq!(parsed.upstream.expect("upstream should exist").ahead, 2);
        assert_eq!(parsed.files.len(), 2);
        assert_eq!(parsed.files[0].state.path, PathBuf::from("file.txt"));
        assert_eq!(parsed.files[1].state.kind, ChangeKind::Untracked);
    }

    #[test]
    fn parses_rename_with_old_path() {
        let status = b"2 R. N... 100644 100644 100644 abc def R100 new.txt\0old.txt\0";

        let parsed = parse_status(status).expect("status should parse");

        assert_eq!(parsed.files[0].state.kind, ChangeKind::Renamed);
        assert_eq!(
            parsed.files[0].state.old_path,
            Some(PathBuf::from("old.txt"))
        );
    }

    #[test]
    fn stages_and_unstages_a_file() {
        let repo = test_repository();
        fs::write(repo.path().join("new.txt"), "new\n").expect("write file");
        let source = super::GitRepositorySource::new(repo.path());

        source
            .apply(&RepositoryAction::Stage(PathBuf::from("new.txt")))
            .expect("stage file");
        assert!(
            source
                .snapshot()
                .expect("staged snapshot")
                .files
                .iter()
                .any(|file| file.path == Path::new("new.txt") && file.staged.is_some())
        );

        source
            .apply(&RepositoryAction::Unstage(PathBuf::from("new.txt")))
            .expect("unstage file");
        let file = source
            .snapshot()
            .expect("unstaged snapshot")
            .files
            .into_iter()
            .find(|file| file.path == Path::new("new.txt"))
            .expect("new file");
        assert_eq!(file.kind, ChangeKind::Untracked);
        assert!(file.staged.is_none());
        assert_eq!(
            file.unstaged.expect("untracked diff").text,
            "@@ -0,0 +1,1 @@\n+new\n"
        );
    }

    #[test]
    fn stages_and_unstages_all_files() {
        let repo = test_repository();
        fs::write(repo.path().join("tracked.txt"), "changed\n").expect("modify file");
        fs::write(repo.path().join("new.txt"), "new\n").expect("write file");
        let source = super::GitRepositorySource::new(repo.path());

        source
            .apply(&RepositoryAction::StageAll)
            .expect("stage all files");
        let snapshot = source.snapshot().expect("snapshot");

        assert_eq!(snapshot.files.len(), 2);
        assert!(snapshot.files.iter().all(|file| file.staged.is_some()));

        source
            .apply(&RepositoryAction::UnstageAll)
            .expect("unstage all files");
        let snapshot = source.snapshot().expect("unstaged snapshot");
        assert_eq!(snapshot.files.len(), 2);
        assert!(snapshot.files.iter().all(|file| file.staged.is_none()));
        assert!(snapshot.files.iter().all(|file| file.unstaged.is_some()));
    }

    #[test]
    fn snapshots_the_whole_untracked_file_as_an_addition() {
        let repo = test_repository();
        fs::write(repo.path().join("new.txt"), "first\nsecond").expect("write file");
        let source = super::GitRepositorySource::new(repo.path());

        let diff = source
            .snapshot()
            .expect("snapshot")
            .files
            .into_iter()
            .find(|file| file.path == Path::new("new.txt"))
            .and_then(|file| file.unstaged)
            .expect("untracked diff");

        assert_eq!(
            diff.text,
            "@@ -0,0 +1,2 @@\n+first\n+second\n\\ No newline at end of file\n"
        );
    }

    #[test]
    fn read_only_source_rejects_actions() {
        let repo = test_repository();
        let source =
            super::GitRepositorySource::new(repo.path()).with_access_mode(AccessMode::ReadOnly);

        let error = source
            .apply(&RepositoryAction::StageAll)
            .expect_err("read-only action should fail");

        assert!(error.to_string().contains("read-only"));
    }

    #[test]
    fn fetches_and_pulls_from_the_configured_remote() {
        let root = tempfile::tempdir().expect("test directory");
        git(root.path(), &["init", "--bare", "remote.git"]);
        git(root.path(), &["clone", "remote.git", "seed"]);
        let seed = root.path().join("seed");
        git(&seed, &["config", "user.name", "Diffo Test"]);
        git(&seed, &["config", "user.email", "diffo@example.invalid"]);
        fs::write(seed.join("base.txt"), "base\n").expect("write base file");
        git(&seed, &["add", "."]);
        git(&seed, &["commit", "-m", "Base commit"]);
        git(&seed, &["push", "-u", "origin", "HEAD"]);
        git(root.path(), &["clone", "remote.git", "work"]);
        let work = root.path().join("work");

        fs::write(seed.join("remote.txt"), "remote\n").expect("write remote file");
        git(&seed, &["add", "."]);
        git(&seed, &["commit", "-m", "Remote commit"]);
        git(&seed, &["push", "origin", "HEAD"]);

        let source = super::GitRepositorySource::new(&work);
        source
            .apply(&RepositoryAction::Fetch)
            .expect("fetch remote");
        assert_eq!(
            source
                .snapshot()
                .expect("fetched snapshot")
                .upstream
                .unwrap()
                .behind,
            1
        );

        source.apply(&RepositoryAction::Pull).expect("pull remote");
        assert!(work.join("remote.txt").exists());
        assert_eq!(
            source
                .snapshot()
                .expect("pulled snapshot")
                .upstream
                .unwrap()
                .behind,
            0
        );
    }

    fn test_repository() -> tempfile::TempDir {
        let repo = tempfile::tempdir().expect("test directory");
        git(repo.path(), &["init", "--initial-branch=main"]);
        git(repo.path(), &["config", "user.name", "Diffo Test"]);
        git(
            repo.path(),
            &["config", "user.email", "diffo@example.invalid"],
        );
        fs::write(repo.path().join("tracked.txt"), "base\n").expect("write tracked file");
        git(repo.path(), &["add", "tracked.txt"]);
        git(repo.path(), &["commit", "-m", "Base commit"]);
        repo
    }

    fn git(repo: &std::path::Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("run git");
        assert!(
            status.status.success(),
            "git failed: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }
}
