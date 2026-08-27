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
    delete_branch::{configure_delete_branch, operation_result as delete_branch_result},
    failure::{classify_failure, finish_sync_command, operation_failure, output_failure},
    refs::{checkout_local_name, ref_exists},
};

mod checkout;
mod protected_push;
mod single_command;
mod sync_target;
pub(super) use checkout::verify_checkout_target;
#[cfg(test)]
pub(super) use protected_push::protected_push_destination;

struct SyncExecution<'a> {
    action: &'a RepositoryAction,
    context: Option<&'a RepositoryOperationContext>,
    cancellation: &'a CancellationHandle,
    bridge: Option<&'a AskpassBridge>,
}

enum SyncRequest<'a> {
    Automatic,
    ToRemote(&'a str),
}

impl SyncRequest<'_> {
    fn selected_remote(&self) -> Option<&str> {
        match self {
            Self::Automatic => None,
            Self::ToRemote(remote) => Some(remote),
        }
    }
}

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

        if let Some(result) = self
            .apply_everyday(action, cancellation)
            .or_else(|| self.apply_merge(action, cancellation))
        {
            return result;
        }

        if let Some(request) = sync_request(action) {
            return self.apply_sync(action, request.selected_remote(), context, cancellation);
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
        single_command::configure(self, &mut command, action)?;

        let _bridge = configure_askpass(&mut command, action, context, self)?;
        let outcome = run_cancellable(&mut command, cancellation)
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        let output = match outcome {
            CommandOutcome::Output(output) => output,
            CommandOutcome::Cancelled => return Ok(OperationOutcome::Cancelled),
        };
        if !output.status.success() {
            return Err(classify_failure(action, &output));
        }
        collect_operation_result(self, action, before_fetch.as_ref())
            .map(OperationOutcome::Completed)
    }

    fn apply_sync(
        &self,
        action: &RepositoryAction,
        selected_remote: Option<&str>,
        context: Option<&RepositoryOperationContext>,
        cancellation: &CancellationHandle,
    ) -> std::result::Result<OperationOutcome, OperationFailure> {
        self.check_sync_starting_state(action)?;
        let branch = self.sync_branch(action)?;
        let target = self.sync_target(action, &branch, selected_remote)?;

        let bridge = context
            .map(AskpassBridge::start)
            .transpose()
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?;
        let execution = SyncExecution {
            action,
            context,
            cancellation,
            bridge: bridge.as_ref(),
        };
        if let Some(context) = context {
            context.progress.progress(SyncProgress::Fetching);
        }
        let mut fetch_args = vec!["fetch"];
        if self
            .is_shallow_repository()
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))?
        {
            fetch_args.push("--unshallow");
        }
        fetch_args.push(&target.remote);
        match self.run_sync_git(action, &fetch_args, cancellation, bridge.as_ref())? {
            CommandOutcome::Cancelled => return Ok(OperationOutcome::Cancelled),
            CommandOutcome::Output(output) if !output.status.success() => {
                return Err(classify_failure(action, &output));
            }
            CommandOutcome::Output(_) => {}
        }

        self.apply_sync_after_fetch(&execution, &branch, &target)
    }

    fn apply_sync_after_fetch(
        &self,
        execution: &SyncExecution<'_>,
        branch: &str,
        target: &sync_target::SyncTarget,
    ) -> std::result::Result<OperationOutcome, OperationFailure> {
        let action = execution.action;
        let (plan, upstream_exists) = self.prepare_sync_plan(action, branch, target)?;
        if let Some(context) = execution.context {
            context.progress.progress(SyncProgress::Plan(plan.clone()));
        }
        if plan.local_only > 0 && plan.upstream_only > 0 {
            self.check_rebase_preconditions(action, &plan)?;
        }
        if self.confirm_protected_push(execution, &plan, target, upstream_exists)? {
            return Ok(OperationOutcome::Cancelled);
        }

        let push_refspec = format!("HEAD:{}", target.upstream_branch);
        let cancelled = if upstream_exists {
            match (plan.local_only, plan.upstream_only) {
                (0, 0) => false,
                (0, _) => self.fast_forward_sync(execution, branch, &target.upstream)?,
                (_, 0) => self.push_sync(
                    execution,
                    &target.remote,
                    &push_refspec,
                    target.establish_upstream,
                )?,
                (_, _) => self.rebase_sync(execution, &plan, target, &push_refspec)?,
            }
        } else {
            self.push_sync(execution, &target.remote, &push_refspec, true)?
        };
        if cancelled {
            return Ok(OperationOutcome::Cancelled);
        }
        if target.establish_upstream && upstream_exists && plan.local_only == 0 {
            self.set_sync_upstream(action, branch, &target.upstream)?;
        }
        Ok(OperationOutcome::Completed(OperationResult::Sync {
            plan: Box::new(plan),
        }))
    }

    fn prepare_sync_plan(
        &self,
        action: &RepositoryAction,
        branch: &str,
        target: &sync_target::SyncTarget,
    ) -> Result<(SyncPlan, bool), OperationFailure> {
        let upstream_exists = self
            .git(&["rev-parse", "--verify", "--quiet", &target.upstream])
            .is_ok();
        if !upstream_exists && !target.establish_upstream {
            return Err(operation_failure(
                action,
                FailureKind::NoUpstream,
                "the configured upstream branch does not exist after fetch",
            ));
        }
        if upstream_exists && target.establish_upstream {
            self.require_related_histories(action, &target.upstream)?;
        }
        let (local_only, upstream_only) = if upstream_exists {
            self.sync_counts_against(action, &target.upstream)?
        } else {
            (self.commit_count(action, "HEAD")?, 0)
        };
        let plan = SyncPlan {
            branch: branch.to_owned(),
            upstream: target.upstream.clone(),
            local_only,
            upstream_only,
            establish_upstream: target.establish_upstream,
        };
        Ok((plan, upstream_exists))
    }

    fn fast_forward_sync(
        &self,
        execution: &SyncExecution<'_>,
        branch: &str,
        upstream: &str,
    ) -> Result<bool, OperationFailure> {
        if let Some(context) = execution.context {
            context.progress.progress(SyncProgress::FastForwarding {
                branch: branch.to_owned(),
            });
        }
        let outcome = self.run_sync_git(
            execution.action,
            &["merge", "--ff-only", "--no-edit", upstream],
            execution.cancellation,
            None,
        )?;
        finish_sync_command(execution.action, outcome)
    }

    fn push_sync(
        &self,
        execution: &SyncExecution<'_>,
        remote: &str,
        refspec: &str,
        set_upstream: bool,
    ) -> Result<bool, OperationFailure> {
        if let Some(context) = execution.context {
            context.progress.progress(SyncProgress::Pushing);
        }
        let mut args = vec!["push", "--porcelain"];
        if set_upstream {
            args.push("--set-upstream");
        }
        args.extend([remote, refspec]);
        let outcome = self.run_sync_git(
            execution.action,
            &args,
            execution.cancellation,
            execution.bridge,
        )?;
        finish_sync_command(execution.action, outcome)
    }

    fn rebase_sync(
        &self,
        execution: &SyncExecution<'_>,
        plan: &SyncPlan,
        target: &sync_target::SyncTarget,
        refspec: &str,
    ) -> Result<bool, OperationFailure> {
        let action = execution.action;
        if let Some(context) = execution.context {
            context.progress.progress(SyncProgress::Rebasing {
                commits: plan.local_only,
            });
        }
        match self.run_sync_git(
            action,
            &["rebase", &plan.upstream],
            execution.cancellation,
            None,
        )? {
            CommandOutcome::Cancelled => {
                self.abort_rebase();
                return Ok(true);
            }
            CommandOutcome::Output(output) if !output.status.success() => {
                let conflicts = self.conflicted_file_count();
                self.abort_rebase();
                if conflicts > 0 {
                    let noun = if conflicts == 1 { "file" } else { "files" };
                    return Err(output_failure(
                        action,
                        FailureKind::RebaseConflict,
                        &format!(
                            "Rebase conflicted in {conflicts} {noun} and was aborted. Nothing was pushed."
                        ),
                        &output,
                    ));
                }
                return Err(output_failure(
                    action,
                    FailureKind::Unknown,
                    "rebase failed and was aborted; nothing was pushed",
                    &output,
                ));
            }
            CommandOutcome::Output(_) => {}
        }
        self.push_sync(
            execution,
            &target.remote,
            refspec,
            target.establish_upstream,
        )
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

    pub(super) fn conflicted_file_count(&self) -> usize {
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

    pub(super) fn git_text(&self, args: &[&str]) -> anyhow::Result<String> {
        self.git(args)
            .map(|output| String::from_utf8_lossy(&output).trim().to_owned())
    }

    fn is_shallow_repository(&self) -> anyhow::Result<bool> {
        match self
            .git_text(&["rev-parse", "--is-shallow-repository"])?
            .as_str()
        {
            "true" => Ok(true),
            "false" => Ok(false),
            value => anyhow::bail!("git returned an invalid shallow repository state: {value}"),
        }
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
        action: &RepositoryAction,
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
        run_cancellable(&mut command, cancellation)
            .map_err(|error| operation_failure(action, FailureKind::Unknown, &error.to_string()))
    }
}

fn sync_request(action: &RepositoryAction) -> Option<SyncRequest<'_>> {
    match action {
        RepositoryAction::Sync => Some(SyncRequest::Automatic),
        RepositoryAction::SyncToRemote(remote) => Some(SyncRequest::ToRemote(remote)),
        _ => None,
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
        RepositoryAction::Sync | RepositoryAction::SyncToRemote(_) => {
            unreachable!("sync collects its result directly")
        }
        RepositoryAction::Commit(_) | RepositoryAction::GuardedCommit(_) => {
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
        RepositoryAction::DeleteBranch(target) => Ok(delete_branch_result(target)),
        RepositoryAction::Merge(_) | RepositoryAction::AbortMerge => {
            unreachable!("merge actions collect their own results")
        }
        RepositoryAction::Discard(_)
        | RepositoryAction::DiscardAll(_)
        | RepositoryAction::Stash { .. }
        | RepositoryAction::ApplyStash(_)
        | RepositoryAction::DropStash(_)
        | RepositoryAction::Amend(_)
        | RepositoryAction::UndoLastCommit(_)
        | RepositoryAction::Revert(_)
        | RepositoryAction::RenameBranch(_) => {
            unreachable!("everyday actions collect their own results")
        }
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
