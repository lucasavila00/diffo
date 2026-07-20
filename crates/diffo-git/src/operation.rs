use std::{
    io,
    os::unix::process::CommandExt as _,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use diffo_core::{
    BranchKind, CancellationHandle, CheckoutTarget, FailureKind, OperationFailure,
    OperationOutcome, OperationResult, RepositoryAction, RepositoryOperationContext,
    RepositorySource, SyncPlan, SyncProgress, UpstreamState,
};
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};

use super::{
    GitRepositorySource,
    askpass::{ASKPASS_MARKER, ASKPASS_SOCKET, AskpassBridge},
    create_branch::{configure_create_branch, operation_result as create_branch_result},
    failure::{classify_failure, command_output, finish_sync_command, operation_failure},
    refs::{checkout_local_name, ref_exists},
};

impl GitRepositorySource {
    pub(super) fn apply_operation(
        &self,
        action: &RepositoryAction,
        context: Option<&RepositoryOperationContext>,
    ) -> std::result::Result<OperationOutcome, OperationFailure> {
        let default_cancellation = CancellationHandle::default();
        let cancellation = context.map_or(&default_cancellation, |context| &context.cancellation);
        if cancellation.is_cancelled() {
            return Ok(OperationOutcome::Cancelled);
        }

        if matches!(action, RepositoryAction::Sync) {
            return self.apply_sync(context, cancellation);
        }

        let before_fetch = matches!(action, RepositoryAction::Fetch)
            .then(|| self.snapshot().ok().and_then(|snapshot| snapshot.upstream))
            .flatten();

        let mut command = Command::new("git");
        command
            .current_dir(&self.root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .stdin(Stdio::null());
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
            RepositoryAction::Sync => unreachable!("sync is handled before single commands"),
            RepositoryAction::Commit(message) => {
                command.args(["commit", "-m", message]);
            }
            RepositoryAction::Checkout(target) => {
                configure_checkout(self, &mut command, action, target)?;
            }
            RepositoryAction::CreateBranch(target) => {
                configure_create_branch(self, &mut command, action, target)?;
            }
        }

        let _bridge = configure_askpass(&mut command, action, context, self)?;
        let outcome = run_cancellable(&mut command, cancellation)
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        let output = match outcome {
            CommandOutcome::Output(output) => output,
            CommandOutcome::Cancelled => return Ok(OperationOutcome::Cancelled),
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(classify_failure(action, &format!("{stdout}\n{stderr}")));
        }
        collect_operation_result(self, action, before_fetch.as_ref())
            .map(OperationOutcome::Completed)
    }

    fn apply_sync(
        &self,
        context: Option<&RepositoryOperationContext>,
        cancellation: &CancellationHandle,
    ) -> std::result::Result<OperationOutcome, OperationFailure> {
        let action = &RepositoryAction::Sync;
        self.check_sync_starting_state(action)?;
        let branch = self.sync_branch(action)?;
        let upstream = self.sync_upstream(action)?;
        let remote = self
            .git_text(&["config", "--get", &format!("branch.{branch}.remote")])
            .map_err(|_| {
                operation_failure(
                    action,
                    FailureKind::NoUpstream,
                    "sync requires a configured upstream",
                )
            })?;
        let upstream_branch = self
            .git_text(&["config", "--get", &format!("branch.{branch}.merge")])
            .map_err(|_| {
                operation_failure(
                    action,
                    FailureKind::NoUpstream,
                    "sync requires a configured upstream",
                )
            })?;
        if !upstream_branch.starts_with("refs/heads/") {
            return Err(operation_failure(
                action,
                FailureKind::NoUpstream,
                "sync requires an upstream branch",
            ));
        }
        let push_refspec = format!("HEAD:{upstream_branch}");

        let bridge = context
            .map(AskpassBridge::start)
            .transpose()
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        if let Some(context) = context {
            context.progress.progress(SyncProgress::Fetching);
        }
        match self.run_sync_git(&["fetch", &remote], cancellation, bridge.as_ref())? {
            CommandOutcome::Cancelled => return Ok(OperationOutcome::Cancelled),
            CommandOutcome::Output(output) if !output.status.success() => {
                return Err(classify_failure(action, &command_output(&output)));
            }
            CommandOutcome::Output(_) => {}
        }

        let (local_only, upstream_only) = self.sync_counts(action)?;
        let plan = SyncPlan {
            branch: branch.clone(),
            upstream: upstream.clone(),
            local_only,
            upstream_only,
        };
        if let Some(context) = context {
            context.progress.progress(SyncProgress::Plan(plan.clone()));
        }

        let cancelled = match (local_only, upstream_only) {
            (0, 0) => false,
            (0, _) => self.fast_forward_sync(&branch, &upstream, context, cancellation)?,
            (_, 0) => self.push_sync(
                &remote,
                &push_refspec,
                context,
                cancellation,
                bridge.as_ref(),
            )?,
            (_, _) => self.rebase_sync(
                &plan,
                &remote,
                &push_refspec,
                context,
                cancellation,
                bridge.as_ref(),
            )?,
        };
        if cancelled {
            return Ok(OperationOutcome::Cancelled);
        }
        Ok(OperationOutcome::Completed(OperationResult::Sync {
            plan: Box::new(plan),
        }))
    }

    fn fast_forward_sync(
        &self,
        branch: &str,
        upstream: &str,
        context: Option<&RepositoryOperationContext>,
        cancellation: &CancellationHandle,
    ) -> Result<bool, OperationFailure> {
        if let Some(context) = context {
            context.progress.progress(SyncProgress::FastForwarding {
                branch: branch.to_owned(),
            });
        }
        let outcome = self.run_sync_git(
            &["merge", "--ff-only", "--no-edit", upstream],
            cancellation,
            None,
        )?;
        finish_sync_command(outcome)
    }

    fn push_sync(
        &self,
        remote: &str,
        refspec: &str,
        context: Option<&RepositoryOperationContext>,
        cancellation: &CancellationHandle,
        bridge: Option<&AskpassBridge>,
    ) -> Result<bool, OperationFailure> {
        if let Some(context) = context {
            context.progress.progress(SyncProgress::Pushing);
        }
        let outcome = self.run_sync_git(
            &["push", "--porcelain", remote, refspec],
            cancellation,
            bridge,
        )?;
        finish_sync_command(outcome)
    }

    fn rebase_sync(
        &self,
        plan: &SyncPlan,
        remote: &str,
        refspec: &str,
        context: Option<&RepositoryOperationContext>,
        cancellation: &CancellationHandle,
        bridge: Option<&AskpassBridge>,
    ) -> Result<bool, OperationFailure> {
        let action = &RepositoryAction::Sync;
        if self.local_only_has_merge_commits(action, &plan.upstream)? {
            return Err(operation_failure(
                action,
                FailureKind::MergeCommits,
                "sync cannot rebase local-only history containing merge commits",
            ));
        }
        if let Some(context) = context {
            context.progress.progress(SyncProgress::Rebasing {
                commits: plan.local_only,
            });
        }
        match self.run_sync_git(&["rebase", &plan.upstream], cancellation, None)? {
            CommandOutcome::Cancelled => {
                self.abort_rebase();
                return Ok(true);
            }
            CommandOutcome::Output(output) if !output.status.success() => {
                let conflicts = self.conflicted_file_count();
                self.abort_rebase();
                if conflicts > 0 {
                    let noun = if conflicts == 1 { "file" } else { "files" };
                    return Err(operation_failure(
                        action,
                        FailureKind::RebaseConflict,
                        &format!(
                            "Rebase conflicted in {conflicts} {noun} and was aborted. Nothing was pushed."
                        ),
                    ));
                }
                return Err(operation_failure(
                    action,
                    FailureKind::Unknown,
                    "rebase failed and was aborted; nothing was pushed",
                ));
            }
            CommandOutcome::Output(_) => {}
        }
        self.push_sync(remote, refspec, context, cancellation, bridge)
    }

    fn sync_branch(&self, action: &RepositoryAction) -> Result<String, OperationFailure> {
        let branch = self
            .git_text(&["symbolic-ref", "--quiet", "--short", "HEAD"])
            .map_err(|_| {
                operation_failure(
                    action,
                    FailureKind::UnsupportedHead,
                    "sync requires an existing local branch; HEAD is detached",
                )
            })?;
        self.git_text(&["rev-parse", "--verify", "HEAD"])
            .map_err(|_| {
                operation_failure(
                    action,
                    FailureKind::UnsupportedHead,
                    "sync requires an existing local branch; the current branch is unborn",
                )
            })?;
        Ok(branch)
    }

    fn sync_upstream(&self, action: &RepositoryAction) -> Result<String, OperationFailure> {
        self.git_text(&[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ])
        .map_err(|_| {
            operation_failure(
                action,
                FailureKind::NoUpstream,
                "sync requires a configured upstream",
            )
        })
    }

    fn check_sync_starting_state(&self, action: &RepositoryAction) -> Result<(), OperationFailure> {
        if ["MERGE_HEAD", "CHERRY_PICK_HEAD"].iter().any(|reference| {
            self.git(&["rev-parse", "--verify", "--quiet", reference])
                .is_ok()
        }) || ["rebase-merge", "rebase-apply"]
            .iter()
            .any(|name| self.git_path(name).is_some_and(|path| path.exists()))
        {
            return Err(operation_failure(
                action,
                FailureKind::OperationInProgress,
                "finish or abort the merge, rebase, or cherry-pick before syncing",
            ));
        }
        let status = self
            .git_text(&["status", "--porcelain"])
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        if !status.is_empty() {
            return Err(operation_failure(
                action,
                FailureKind::DirtyWorktree,
                "sync currently requires a clean worktree and index",
            ));
        }
        Ok(())
    }

    fn sync_counts(&self, action: &RepositoryAction) -> Result<(usize, usize), OperationFailure> {
        let counts = self
            .git_text(&["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        let mut counts = counts.split_whitespace();
        let local_only = counts.next().and_then(|value| value.parse().ok());
        let upstream_only = counts.next().and_then(|value| value.parse().ok());
        match (local_only, upstream_only, counts.next()) {
            (Some(local_only), Some(upstream_only), None) => Ok((local_only, upstream_only)),
            _ => Err(operation_failure(
                action,
                FailureKind::Unknown,
                "git returned invalid sync commit counts",
            )),
        }
    }

    fn local_only_has_merge_commits(
        &self,
        action: &RepositoryAction,
        upstream: &str,
    ) -> Result<bool, OperationFailure> {
        let range = format!("{upstream}..HEAD");
        self.git_text(&["rev-list", "--min-parents=2", "--max-count=1", &range])
            .map(|output| !output.is_empty())
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))
    }

    fn conflicted_file_count(&self) -> usize {
        self.git_text(&["diff", "--name-only", "--diff-filter=U"])
            .map_or(0, |paths| paths.lines().count())
    }

    fn abort_rebase(&self) {
        let _ = Command::new("git")
            .args(["rebase", "--abort"])
            .current_dir(&self.root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .stdin(Stdio::null())
            .output();
    }

    fn git_text(&self, args: &[&str]) -> anyhow::Result<String> {
        self.git(args)
            .map(|output| String::from_utf8_lossy(&output).trim().to_owned())
    }

    fn git_path(&self, name: &str) -> Option<std::path::PathBuf> {
        self.git_text(&["rev-parse", "--git-path", name])
            .ok()
            .map(std::path::PathBuf::from)
            .map(|path| {
                if path.is_absolute() {
                    path
                } else {
                    self.root.join(path)
                }
            })
    }

    fn run_sync_git(
        &self,
        args: &[&str],
        cancellation: &CancellationHandle,
        bridge: Option<&AskpassBridge>,
    ) -> Result<CommandOutcome, OperationFailure> {
        let mut command = Command::new("git");
        command
            .args(args)
            .current_dir(&self.root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .stdin(Stdio::null());
        if let Some(bridge) = bridge {
            let executable = self.askpass_executable().ok_or_else(|| {
                operation_failure(
                    &RepositoryAction::Sync,
                    FailureKind::Unknown,
                    "askpass executable is unavailable",
                )
            })?;
            command
                .env("GIT_ASKPASS", executable)
                .env("SSH_ASKPASS", executable)
                .env("SSH_ASKPASS_REQUIRE", "force")
                .env(ASKPASS_MARKER, "1")
                .env(ASKPASS_SOCKET, bridge.socket());
        }
        run_cancellable(&mut command, cancellation).map_err(|error| {
            operation_failure(
                &RepositoryAction::Sync,
                FailureKind::Unknown,
                &error.to_string(),
            )
        })
    }
}

fn configure_askpass(
    command: &mut Command,
    action: &RepositoryAction,
    context: Option<&RepositoryOperationContext>,
    source: &GitRepositorySource,
) -> std::result::Result<Option<AskpassBridge>, OperationFailure> {
    if !matches!(action, RepositoryAction::Fetch) {
        return Ok(None);
    }
    let bridge = context
        .map(AskpassBridge::start)
        .transpose()
        .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
    if let Some(bridge) = bridge.as_ref() {
        let executable = source.askpass_executable().ok_or_else(|| {
            operation_failure(
                action,
                FailureKind::Unknown,
                "askpass executable is unavailable",
            )
        })?;
        command
            .env("GIT_ASKPASS", executable)
            .env("SSH_ASKPASS", executable)
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env(ASKPASS_MARKER, "1")
            .env(ASKPASS_SOCKET, bridge.socket());
    }
    Ok(bridge)
}

pub(super) enum CommandOutcome {
    Output(Output),
    Cancelled,
}

pub(super) fn run_cancellable(
    command: &mut Command,
    cancellation: &CancellationHandle,
) -> io::Result<CommandOutcome> {
    if cancellation.is_cancelled() {
        return Ok(CommandOutcome::Cancelled);
    }
    command
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().map(read_output);
    let stderr = child.stderr.take().map(read_output);
    let mut was_cancelled = false;
    let status = loop {
        if cancellation.is_cancelled() && !was_cancelled {
            was_cancelled = true;
            signal_process_group(child.id(), Signal::SIGTERM);
        }
        if let Some(status) = child.try_wait()? {
            was_cancelled |= cancellation.is_cancelled();
            break status;
        }
        if was_cancelled {
            let deadline = Instant::now() + Duration::from_millis(200);
            while Instant::now() < deadline {
                if let Some(status) = child.try_wait()? {
                    let _ = join_output(stdout);
                    let _ = join_output(stderr);
                    let _ = status;
                    return Ok(CommandOutcome::Cancelled);
                }
                thread::sleep(Duration::from_millis(10));
            }
            signal_process_group(child.id(), Signal::SIGKILL);
            let _ = child.wait();
            let _ = join_output(stdout);
            let _ = join_output(stderr);
            return Ok(CommandOutcome::Cancelled);
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = join_output(stdout)?;
    let stderr = join_output(stderr)?;
    if was_cancelled {
        return Ok(CommandOutcome::Cancelled);
    }
    Ok(CommandOutcome::Output(Output {
        status,
        stdout,
        stderr,
    }))
}

fn read_output(
    mut pipe: impl io::Read + Send + 'static,
) -> thread::JoinHandle<io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut output = Vec::new();
        pipe.read_to_end(&mut output)?;
        Ok(output)
    })
}

fn join_output(reader: Option<thread::JoinHandle<io::Result<Vec<u8>>>>) -> io::Result<Vec<u8>> {
    reader.map_or_else(
        || Ok(Vec::new()),
        |reader| {
            reader
                .join()
                .map_err(|_| io::Error::other("Git output reader stopped"))?
        },
    )
}

fn signal_process_group(id: u32, signal: Signal) {
    if let Ok(id) = i32::try_from(id) {
        let _ = killpg(Pid::from_raw(id), signal);
    }
}

fn collect_operation_result(
    source: &GitRepositorySource,
    action: &RepositoryAction,
    before_fetch: Option<&UpstreamState>,
) -> std::result::Result<OperationResult, OperationFailure> {
    match action {
        RepositoryAction::Stage(_) | RepositoryAction::StageAll => Ok(OperationResult::Stage),
        RepositoryAction::Unstage(_) | RepositoryAction::UnstageAll => Ok(OperationResult::Unstage),
        RepositoryAction::Fetch => {
            let after = source
                .snapshot()
                .ok()
                .and_then(|snapshot| snapshot.upstream);
            let updated_refs = usize::from(before_fetch != after.as_ref());
            Ok(OperationResult::Fetch { updated_refs })
        }
        RepositoryAction::Sync => unreachable!("sync collects its result directly"),
        RepositoryAction::Commit(_) => {
            let hash = source
                .git(&["rev-parse", "HEAD"])
                .map_err(|error| {
                    operation_failure(action, FailureKind::Unknown, &error.to_string())
                })
                .map(|head| String::from_utf8_lossy(&head).trim().to_owned())?;
            Ok(OperationResult::Commit { hash })
        }
        RepositoryAction::Checkout(target) => Ok(OperationResult::Checkout {
            branch: checkout_local_name(action, target)?,
        }),
        RepositoryAction::CreateBranch(target) => Ok(create_branch_result(target)),
    }
}

fn configure_checkout(
    source: &GitRepositorySource,
    command: &mut Command,
    action: &RepositoryAction,
    target: &CheckoutTarget,
) -> std::result::Result<(), OperationFailure> {
    verify_checkout_target(source, action, target)?;
    let local_name = checkout_local_name(action, target)?;
    match target.kind {
        BranchKind::Local => {
            command.args(["checkout", "--no-guess", &local_name]);
        }
        BranchKind::Remote => {
            let local_ref = format!("refs/heads/{local_name}");
            if ref_exists(source, &local_ref).map_err(|error| {
                operation_failure(action, FailureKind::Unknown, &error.to_string())
            })? {
                let upstream = source
                    .git(&[
                        "for-each-ref",
                        "--format=%(upstream)",
                        "--count=1",
                        &local_ref,
                    ])
                    .map_err(|error| {
                        operation_failure(action, FailureKind::Unknown, &error.to_string())
                    })?;
                let upstream = String::from_utf8(upstream)
                    .map_err(|_| {
                        operation_failure(
                            action,
                            FailureKind::Unknown,
                            "git returned a non-UTF-8 upstream",
                        )
                    })?
                    .trim()
                    .to_owned();
                if upstream != target.full_ref {
                    return Err(operation_failure(
                        action,
                        FailureKind::BranchConflict,
                        "a local branch with that name tracks a different upstream",
                    ));
                }
                command.args(["checkout", "--no-guess", &local_name]);
            } else {
                command.args([
                    "checkout",
                    "--no-guess",
                    "--track",
                    "-b",
                    &local_name,
                    &target.full_ref,
                ]);
            }
        }
    }
    Ok(())
}

pub(super) fn verify_checkout_target(
    source: &GitRepositorySource,
    action: &RepositoryAction,
    target: &CheckoutTarget,
) -> std::result::Result<(), OperationFailure> {
    let expected_prefix = match target.kind {
        BranchKind::Local => "refs/heads/",
        BranchKind::Remote => "refs/remotes/",
    };
    if !target.full_ref.starts_with(expected_prefix) {
        return Err(operation_failure(
            action,
            FailureKind::RefChanged,
            "selected branch is no longer available; reopen the branch picker",
        ));
    }
    let object_id = source
        .git(&["show-ref", "--verify", "--hash", &target.full_ref])
        .ok()
        .and_then(|output| String::from_utf8(output).ok())
        .map(|output| output.trim().to_owned());
    if object_id.as_deref() != Some(target.object_id.as_str()) {
        return Err(operation_failure(
            action,
            FailureKind::RefChanged,
            "selected branch changed; reopen the branch picker",
        ));
    }
    Ok(())
}
