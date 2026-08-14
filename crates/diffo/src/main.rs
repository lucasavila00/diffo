#![doc = include_str!("../README.md")]

use std::{
    env, fmt, fs,
    io::{self, Write as _},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
#[cfg(windows)]
use crossterm::event::EnableMouseCapture;
use crossterm::{
    Command,
    event::{self, DisableMouseCapture, Event},
    execute,
    terminal::{Clear, ClearType},
};
use diffo_app::ToastKind;
use diffo_core::{
    FailureKind, OperationFailure, Repository, RepositoryUpdateKind,
    fixture_source::MutableFixtureRepository,
};
use diffo_git::{GitRepositorySource, NotRepository, run_askpass_if_requested};
use diffo_repository_service::{RepositoryEvent, RepositoryService};
use ratatui::{Terminal, backend::CrosstermBackend};

mod codex_tasks;
mod frame_trace;
mod history_requests;
mod launcher;
mod merge;
mod startup;
mod tool_tasks;
mod update_tasks;
mod wheel_friction;

use codex_tasks::CodexTasks;
use diffo_app::workbench::{PromptResponse, Workbench, WorkbenchEffect};
use frame_trace::{FrameRecord, FrameTracer, input_events as trace_input_events};
use startup::{StartupPhase, StartupReporter};
use tool_tasks::ToolTasks;
use update_tasks::UpdateTasks;
use wheel_friction::{WheelFriction, filter as filter_wheel_momentum};

struct RuntimeTasks {
    tools: ToolTasks,
    codex: CodexTasks,
    updates: UpdateTasks,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EnableActionMouseCapture;

impl Command for EnableActionMouseCapture {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str(concat!(
            // Press and release events.
            "\x1b[?1000h",
            // Movement events only while a button is held, for dragging.
            "\x1b[?1002h",
            // Extended coordinates, with SGR preferred over RXVT mode.
            "\x1b[?1015h",
            "\x1b[?1006h",
        ))
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> io::Result<()> {
        Command::execute_winapi(&EnableMouseCapture)
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        Command::is_ansi_code_supported(&EnableMouseCapture)
    }
}

fn main() -> Result<()> {
    if let Some(status) = run_askpass_if_requested() {
        std::process::exit(status);
    }
    match launcher::dispatch(env::args_os().skip(1))? {
        launcher::LaunchMode::Update => {
            run_updater();
            Ok(())
        }
        launcher::LaunchMode::Application => match run_application() {
            Err(error) if error.downcast_ref::<NotRepository>().is_some() => {
                eprintln!("Diffo must be run inside a Git repository.");
                std::process::exit(1);
            }
            result => result,
        },
    }
}

fn run_updater() {
    let client = match diffo_update::UpdateClient::from_environment(diffo_app::BUILD_SHA) {
        Ok(client) => client,
        Err(error) => {
            print_update_error(&error);
            std::process::exit(1);
        }
    };
    match client.install_latest() {
        Ok(diffo_update::InstallOutcome::UpToDate { current, latest }) => {
            println!("Diffo is up to date (commit {current}; latest commit {latest}).");
        }
        Ok(diffo_update::InstallOutcome::Installed {
            previous,
            installed,
        }) => println!(
            "Updated Diffo from commit {previous} to commit {installed}. Quit and relaunch Diffo to use the new version."
        ),
        Err(error) => {
            print_update_error(&error);
            std::process::exit(1);
        }
    }
}

fn print_update_error(error: &diffo_update::UpdateError) {
    let label = match error.category() {
        diffo_update::ErrorCategory::Network => "Update network error",
        diffo_update::ErrorCategory::Verification => "Update verification failed",
        diffo_update::ErrorCategory::Permission => "Update permission error",
        diffo_update::ErrorCategory::Other => "Update failed",
    };
    eprintln!("{label}: {error}");
    if error.category() == diffo_update::ErrorCategory::Permission
        && let Some(command) = diffo_update::permission_hint()
    {
        eprintln!("Retry with: {command}");
    }
}

fn run_application() -> Result<()> {
    let shutdown = install_signal_handlers()?;
    let mock_path = env::var_os("DIFFO_MOCK_FILE");
    let startup = StartupReporter::start(if mock_path.is_some() {
        StartupPhase::LoadingMockRepository
    } else {
        StartupPhase::FindingGitRepository
    });
    let (repository, watch_paths): (
        Arc<dyn Repository>,
        Option<diffo_core::RepositoryWatchPaths>,
    ) = if let Some(path) = mock_path {
        (
            Arc::new(MutableFixtureRepository::new_with_large_files(path)?),
            None,
        )
    } else {
        let repository = Arc::new(GitRepositorySource::discover_with_askpass(".")?);
        startup.phase(StartupPhase::ResolvingRepositoryPaths);
        let paths = repository.watch_paths()?;
        (repository, Some(paths))
    };
    startup.phase(StartupPhase::ReadingRepositoryState);
    let snapshot = repository.snapshot()?;
    if let Some(path) = env::var_os("DIFFO_DUMP_PATH") {
        startup.finish();
        return dump_snapshot(Path::new(&path), &snapshot);
    }
    startup.phase(StartupPhase::StartingRepositoryServices);
    let repository_service =
        RepositoryService::start(Arc::clone(&repository), watch_paths.as_ref())?;
    if let Some(path) = env::var_os("DIFFO_WATCH_DUMP_PATH") {
        startup.finish();
        return run_watch_dump(Path::new(&path), &snapshot, &repository_service, &shutdown);
    }

    startup.phase(StartupPhase::PreparingInterface);
    let repository_root = match &watch_paths {
        Some(paths) => paths.worktree.clone(),
        None => env::current_dir().context("failed to resolve the repository directory")?,
    };

    let mut workbench = Workbench::new(snapshot);
    let mut tasks = RuntimeTasks {
        tools: ToolTasks::start(repository),
        codex: CodexTasks::new(repository_root),
        updates: UpdateTasks::new(),
    };
    tasks.tools.drain(&mut workbench);
    let mut tracer = FrameTracer::from_environment();
    startup.finish();
    let mut terminal = ratatui::init();
    execute!(
        terminal.backend_mut(),
        Clear(ClearType::Purge),
        EnableActionMouseCapture
    )?;

    let result = run(
        &mut terminal,
        &mut workbench,
        &shutdown,
        &repository_service,
        &mut tasks,
        &mut tracer,
    );
    drop(repository_service);
    let mouse_result = execute!(terminal.backend_mut(), DisableMouseCapture)
        .context("failed to disable mouse capture");
    ratatui::restore();
    result.and(mouse_result)
}

fn install_signal_handlers() -> Result<Arc<AtomicBool>> {
    let shutdown = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&shutdown))
        .context("failed to register SIGINT handler")?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&shutdown))
        .context("failed to register SIGTERM handler")?;
    Ok(shutdown)
}

fn dump_snapshot(path: &Path, snapshot: &diffo_core::RepositorySnapshot) -> Result<()> {
    let contents = ron::ser::to_string_pretty(snapshot, ron::ser::PrettyConfig::default())
        .context("failed to serialize repository snapshot")?;
    fs::write(path, contents)
        .with_context(|| format!("failed to write repository snapshot to {}", path.display()))
}

fn dump_snapshot_atomic(path: &Path, snapshot: &diffo_core::RepositorySnapshot) -> Result<()> {
    let temporary = path.with_extension("tmp");
    dump_snapshot(&temporary, snapshot)?;
    fs::rename(&temporary, path)
        .with_context(|| format!("failed to publish snapshot dump to {}", path.display()))
}

fn run_watch_dump(
    path: &Path,
    initial: &diffo_core::RepositorySnapshot,
    repository_service: &RepositoryService,
    shutdown: &AtomicBool,
) -> Result<()> {
    dump_snapshot_atomic(path, initial)?;
    let mut generation = 0;
    while !shutdown.load(Ordering::Relaxed) {
        while let Ok(Some(event)) = repository_service.try_recv() {
            match event {
                RepositoryEvent::Update(update) if update.generation > generation => {
                    generation = update.generation;
                    match update.kind {
                        RepositoryUpdateKind::Snapshot(snapshot) => {
                            dump_snapshot_atomic(path, &snapshot)?;
                        }
                        RepositoryUpdateKind::RefreshFailed(message) => eprintln!("{message}"),
                        RepositoryUpdateKind::CommandCompleted { .. }
                        | RepositoryUpdateKind::CommandFailed { .. }
                        | RepositoryUpdateKind::CommandCancelled { .. } => {}
                    }
                }
                RepositoryEvent::Prompt {
                    command_id,
                    prompt_id,
                    ..
                } => {
                    let _ = repository_service.answer_prompt(
                        command_id,
                        prompt_id,
                        diffo_core::PromptAnswer::Cancel,
                    );
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    workbench: &mut Workbench,
    shutdown: &AtomicBool,
    repository_service: &RepositoryService,
    tasks: &mut RuntimeTasks,
    tracer: &mut FrameTracer,
) -> Result<()> {
    let mut wheel_friction = WheelFriction::default();
    let scroll = (
        workbench.diff_model().diff_scroll,
        workbench.diff_model().diff_horizontal_scroll,
    );
    let update_start_us = tracer.elapsed_us();
    let preparation = prepare_frame(terminal, workbench)?;
    let (draw_start_us, draw_end_us) = draw_frame(terminal, workbench, tracer)?;
    let _ = workbench.take_redraw_request();
    tracer.record(FrameRecord::new(
        Vec::new(),
        workbench.protected_push_prompt_open(),
        workbench.modal_trace_label(),
        workbench.repository_generation(),
        workbench.diff_model(),
        &preparation,
        scroll,
        update_start_us,
        None,
        draw_start_us,
        draw_end_us,
    ));
    let mut pending_trace_events = Vec::new();
    while !workbench.should_quit() && !shutdown.load(Ordering::Relaxed) {
        workbench.tick(Instant::now());
        let poll_timeout = if workbench.is_preparing()
            || repository_service.is_busy()
            || workbench.has_active_command()
        {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(50)
        };
        let mut events = Vec::new();
        let mut event_read_us = None;
        if event::poll(poll_timeout)? {
            event_read_us = Some(tracer.elapsed_us());
            events.push(event::read()?);
            while event::poll(Duration::ZERO)? {
                events.push(event::read()?);
            }
        }
        let input_events = trace_input_events(&events, workbench.secret_prompt_open());
        pending_trace_events.extend(input_events);
        let input_time = Instant::now();
        filter_wheel_momentum(&mut events, workbench, &mut wheel_friction, input_time);
        let scroll_before = (
            workbench.diff_model().diff_scroll,
            workbench.diff_model().diff_horizontal_scroll,
        );
        let update_start_us = tracer.elapsed_us();
        drain_repository_events(repository_service, workbench);
        drain_tasks(tasks, workbench, repository_service);
        dispatch_events(&events, terminal, workbench, repository_service, tasks)?;
        let preparation = prepare_frame(terminal, workbench)?;
        let resized = events
            .iter()
            .any(|event| matches!(event, Event::Resize(_, _)));
        let redraw_requested = workbench.take_redraw_request();
        if !resized && !redraw_requested {
            if !pending_trace_events.is_empty() {
                record_suppressed_input(
                    tracer,
                    workbench,
                    &preparation,
                    &mut pending_trace_events,
                    scroll_before,
                    update_start_us,
                    event_read_us,
                );
            }
            continue;
        }
        let (draw_start_us, draw_end_us) = draw_frame(terminal, workbench, tracer)?;
        tracer.record(FrameRecord::new(
            std::mem::take(&mut pending_trace_events),
            workbench.protected_push_prompt_open(),
            workbench.modal_trace_label(),
            workbench.repository_generation(),
            workbench.diff_model(),
            &preparation,
            scroll_before,
            update_start_us,
            event_read_us,
            draw_start_us,
            draw_end_us,
        ));
    }

    if let Some(command_id) = workbench.active_command_id() {
        let _ = workbench.cancel_application_command(command_id);
        let _ = repository_service.cancel_command(command_id);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn record_suppressed_input(
    tracer: &mut FrameTracer,
    workbench: &Workbench,
    preparation: &diffo_app::FramePreparation,
    input_events: &mut Vec<String>,
    scroll_before: (usize, usize),
    update_start_us: u64,
    event_read_us: Option<u64>,
) {
    let timestamp_us = tracer.elapsed_us();
    tracer.record(FrameRecord::suppressed(
        std::mem::take(input_events),
        workbench.protected_push_prompt_open(),
        workbench.modal_trace_label(),
        workbench.repository_generation(),
        workbench.diff_model(),
        preparation,
        scroll_before,
        update_start_us,
        event_read_us,
        timestamp_us,
    ));
}

fn draw_frame(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    workbench: &mut Workbench,
    tracer: &FrameTracer,
) -> Result<(u64, u64)> {
    let draw_start_us = tracer.elapsed_us();
    terminal.draw(|frame| workbench.render(frame))?;
    let draw_end_us = tracer.elapsed_us();
    Ok((draw_start_us, draw_end_us))
}

fn prepare_frame(
    terminal: &Terminal<CrosstermBackend<io::Stdout>>,
    workbench: &mut Workbench,
) -> Result<diffo_app::FramePreparation> {
    let size = terminal.size()?;
    let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
    Ok(workbench.prepare_frame(area))
}

fn drain_tasks(
    tasks: &mut RuntimeTasks,
    workbench: &mut Workbench,
    repository_service: &RepositoryService,
) {
    tasks.tools.drain(workbench);
    tasks.codex.drain(workbench, repository_service);
    tasks.updates.drain(workbench);
}

fn dispatch_events(
    events: &[crossterm::event::Event],
    terminal: &Terminal<CrosstermBackend<io::Stdout>>,
    workbench: &mut Workbench,
    repository_service: &RepositoryService,
    tasks: &RuntimeTasks,
) -> Result<()> {
    let size = terminal.size()?;
    let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
    for effect in workbench.handle_events(events, area) {
        dispatch_effect(effect, workbench, repository_service);
    }
    while let Some(query_id) = workbench.take_branch_query() {
        if !repository_service.load_branches(query_id) {
            workbench.branches_load_failed(query_id, "repository service is unavailable");
        }
    }
    history_requests::dispatch(workbench, repository_service);
    merge::dispatch_queries(workbench, repository_service);
    while let Some(query_id) = workbench.take_sync_remote_query() {
        if !repository_service.load_remotes(query_id) {
            workbench.sync_remotes_load_failed(query_id, "repository service is unavailable");
        }
    }
    let command_start = Instant::now();
    while let Some(command) = workbench.take_application_command(command_start) {
        let id = command.id;
        match command.action {
            diffo_app::workbench::ApplicationAction::Repository(action) => {
                if !repository_service.execute(id, action.clone(), command.cancellation) {
                    workbench.action_failed(
                        id,
                        OperationFailure {
                            action,
                            kind: FailureKind::Unknown,
                            detail: "repository service is unavailable".to_owned(),
                        },
                    );
                }
            }
            diffo_app::workbench::ApplicationAction::AiCommit(request) => {
                tasks.codex.start(id, request, command.cancellation);
            }
            diffo_app::workbench::ApplicationAction::Update => {
                tasks.updates.start_update(id, command.cancellation);
            }
        }
    }
    Ok(())
}

fn dispatch_effect(
    effect: WorkbenchEffect,
    workbench: &mut Workbench,
    repository_service: &RepositoryService,
) {
    match effect {
        WorkbenchEffect::CopyPath { path, absolute } => {
            match copy_path_to_clipboard(&path, absolute) {
                Ok(copied) => {
                    workbench.show_toast(
                        ToastKind::Success,
                        format!(
                            "Copied {} path: {copied}",
                            if absolute { "absolute" } else { "relative" }
                        ),
                    );
                }
                Err(error) => {
                    workbench.show_error("Could not copy path", error.to_string());
                }
            }
        }
        WorkbenchEffect::Prompt {
            command_id,
            prompt_id,
            response,
        } => {
            let cancelled = matches!(response, PromptResponse::Cancel);
            let answer = match response {
                PromptResponse::Text(answer) => diffo_core::PromptAnswer::Text(answer),
                PromptResponse::Confirm => diffo_core::PromptAnswer::Confirm,
                PromptResponse::Cancel => diffo_core::PromptAnswer::Cancel,
            };
            if !repository_service.answer_prompt(command_id, prompt_id, answer) || cancelled {
                let _ = repository_service.cancel_command(command_id);
            }
        }
    }
}

fn drain_repository_events(repository_service: &RepositoryService, workbench: &mut Workbench) {
    while let Ok(Some(event)) = repository_service.try_recv() {
        match event {
            RepositoryEvent::WorktreeChanged => workbench.filesystem_changed(),
            RepositoryEvent::HistoryLoaded { query_id, history } => {
                workbench.history_loaded(query_id, history);
            }
            RepositoryEvent::HistoryLoadFailed { query_id, message } => {
                workbench.history_load_failed(query_id, &message);
            }
            RepositoryEvent::CommitPatchLoaded {
                query_id,
                commit_id,
                patch,
            } => workbench.commit_patch_loaded(query_id, commit_id, patch),
            RepositoryEvent::CommitPatchLoadFailed {
                query_id,
                commit_id,
                message,
            } => workbench.commit_patch_load_failed(query_id, &commit_id, &message),
            RepositoryEvent::BranchesLoaded { query_id, branches } => {
                workbench.branches_loaded(query_id, branches);
            }
            RepositoryEvent::BranchesLoadFailed { query_id, message } => {
                workbench.branches_load_failed(query_id, &message);
            }
            RepositoryEvent::MergeRefsLoaded { query_id, refs } => {
                workbench.merge_refs_loaded(query_id, refs);
            }
            RepositoryEvent::MergeRefsLoadFailed { query_id, message } => {
                workbench.merge_refs_load_failed(query_id, &message);
            }
            RepositoryEvent::RemotesLoaded { query_id, remotes } => {
                workbench.sync_remotes_loaded(query_id, remotes);
            }
            RepositoryEvent::RemotesLoadFailed { query_id, message } => {
                workbench.sync_remotes_load_failed(query_id, &message);
            }
            RepositoryEvent::StashesLoaded { .. } | RepositoryEvent::StashesLoadFailed { .. } => {}
            RepositoryEvent::Prompt {
                command_id,
                prompt_id,
                prompt,
            } => {
                if !workbench.open_prompt(command_id, prompt_id, prompt) {
                    let _ = repository_service.answer_prompt(
                        command_id,
                        prompt_id,
                        diffo_core::PromptAnswer::Cancel,
                    );
                    let _ = repository_service.cancel_command(command_id);
                }
            }
            RepositoryEvent::Progress {
                command_id,
                progress,
            } => workbench.accept_sync_progress(command_id, progress),
            RepositoryEvent::Update(update) => {
                let _ = workbench.accept_repository_update(update);
            }
        }
    }
}

fn copy_path_to_clipboard(path: &Path, absolute: bool) -> Result<String> {
    let value = if absolute {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .context("failed to find repository root")?;
        if !output.status.success() {
            anyhow::bail!("Git repository root is unavailable");
        }
        let root = String::from_utf8(output.stdout).context("repository root is not UTF-8")?;
        Path::new(root.trim()).join(path).display().to_string()
    } else {
        path.display().to_string()
    };
    if env::var_os("TMUX").is_some() && copy_with_tmux(&value).is_ok() {
        return Ok(value);
    }
    let encoded = BASE64.encode(value.as_bytes());
    let osc52 = format!("\x1b]52;c;{encoded}\x07");
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(osc52.as_bytes())
        .context("failed to write OSC 52 clipboard escape")?;
    if env::var("TERM").is_ok_and(|term| term.starts_with("screen")) {
        // Across SSH only TERM survives. Byobu may use GNU Screen or tmux, so
        // send both passthrough forms; unsupported DCS forms are ignored.
        write!(stdout, "\x1bP{osc52}\x1b\\")
            .context("failed to write GNU Screen clipboard passthrough")?;
        let tmux_payload = osc52.replace('\x1b', "\x1b\x1b");
        write!(stdout, "\x1bPtmux;{tmux_payload}\x1b\\")
            .context("failed to write tmux clipboard passthrough")?;
    }
    stdout.flush().context("failed to flush clipboard escape")?;
    Ok(value)
}

fn copy_with_tmux(value: &str) -> Result<()> {
    let mut child = std::process::Command::new("tmux")
        .args(["load-buffer", "-w", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("failed to start tmux clipboard command")?;
    child
        .stdin
        .take()
        .context("tmux clipboard stdin is unavailable")?
        .write_all(value.as_bytes())
        .context("failed to send path to tmux")?;
    let status = child.wait().context("failed to wait for tmux clipboard")?;
    if !status.success() {
        anyhow::bail!("tmux clipboard command failed");
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::{EnableActionMouseCapture, trace_input_events};
    use crossterm::{
        Command,
        event::{Event, KeyModifiers},
    };

    #[test]
    fn mouse_capture_requests_only_actionable_events() {
        let mut sequence = String::new();
        EnableActionMouseCapture.write_ansi(&mut sequence).unwrap();

        assert_eq!(
            sequence,
            concat!("\x1b[?1000h", "\x1b[?1002h", "\x1b[?1015h", "\x1b[?1006h")
        );
        assert_eq!(sequence.len(), 32, "mouse setup has a fixed byte budget");
        assert!(!sequence.contains("\x1b[?1003h"));
    }

    #[test]
    fn secret_prompt_input_is_redacted_from_frame_traces() {
        let sentinel = "sentinel-secret";
        let events = sentinel
            .chars()
            .map(|character| {
                Event::Key(crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Char(character),
                    KeyModifiers::NONE,
                ))
            })
            .collect::<Vec<_>>();

        let redacted = trace_input_events(&events, true).join("");
        assert!(!redacted.contains(sentinel));
        assert!(redacted.contains("[redacted]"));
    }
}
