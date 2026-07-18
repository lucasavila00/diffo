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
    event::{self, DisableMouseCapture, Event, MouseEventKind},
    execute,
    terminal::{Clear, ClearType},
};
use diffo_app::ToastKind;
use diffo_core::{
    FailureKind, OperationFailure, Repository, fixture_source::MutableFixtureRepository,
};
use diffo_git::{GitRepositorySource, run_askpass_if_requested};
use diffo_repository_service::{RepositoryEvent, RepositoryService};
use ratatui::{Terminal, backend::CrosstermBackend};

mod frame_trace;
mod tool_tasks;

use diffo_workbench::{PromptResponse, Workbench, WorkbenchEffect};
use frame_trace::{FrameRecord, FrameTracer};
use tool_tasks::ToolTasks;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EnableActionMouseCapture;

const WHEEL_ACTIVE_INTERVAL: Duration = Duration::from_millis(48);
const WHEEL_BURST_RESET: Duration = Duration::from_millis(120);

#[derive(Default)]
struct WheelFriction {
    direction: Option<MouseEventKind>,
    last_event: Option<Instant>,
}

impl WheelFriction {
    fn accepts(&mut self, event: &Event, now: Instant) -> bool {
        let Event::Mouse(mouse) = event else {
            return true;
        };
        let direction = match mouse.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => mouse.kind,
            _ => return true,
        };
        let gap = self.last_event.map(|last| now.duration_since(last));
        let starts_burst =
            self.direction != Some(direction) || gap.is_none_or(|gap| gap >= WHEEL_BURST_RESET);
        self.last_event = Some(now);
        if starts_burst {
            self.direction = Some(direction);
            return true;
        }
        gap.is_some_and(|gap| gap <= WHEEL_ACTIVE_INTERVAL)
    }
}

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
    let shutdown = install_signal_handlers()?;
    let (repository, watch_paths): (Arc<dyn Repository>, Option<Vec<_>>) =
        if let Some(path) = env::var_os("DIFFO_MOCK_FILE") {
            (
                Arc::new(MutableFixtureRepository::new_with_large_files(path)?),
                None,
            )
        } else {
            let repository = Arc::new(GitRepositorySource::default());
            let paths = repository.watch_paths()?;
            (repository, Some(paths))
        };
    let snapshot = repository.snapshot()?;
    if let Some(path) = env::var_os("DIFFO_DUMP_PATH") {
        return dump_snapshot(Path::new(&path), &snapshot);
    }
    let repository_service =
        RepositoryService::start(Arc::clone(&repository), watch_paths.as_deref())?;
    if let Some(path) = env::var_os("DIFFO_WATCH_DUMP_PATH") {
        return run_watch_dump(Path::new(&path), &snapshot, &repository_service, &shutdown);
    }

    let mut workbench = Workbench::new(snapshot);
    let tool_tasks = ToolTasks::start(Arc::clone(&repository));
    tool_tasks.drain(&mut workbench);
    let mut tracer = FrameTracer::from_environment();
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
        &tool_tasks,
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
                RepositoryEvent::SnapshotRefreshed {
                    generation: next,
                    snapshot,
                } if next > generation => {
                    generation = next;
                    dump_snapshot_atomic(path, &snapshot)?;
                }
                RepositoryEvent::RefreshFailed { message, .. } => eprintln!("{message}"),
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
                RepositoryEvent::SnapshotRefreshed { .. }
                | RepositoryEvent::CommandCompleted { .. }
                | RepositoryEvent::CommandFailed { .. }
                | RepositoryEvent::CommandCancelled { .. } => {}
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
    tool_tasks: &ToolTasks,
    tracer: &mut FrameTracer,
) -> Result<()> {
    let mut generation = 0;
    let mut wheel_friction = WheelFriction::default();
    let scroll = (
        workbench.diff_model().diff_scroll,
        workbench.diff_model().diff_horizontal_scroll,
    );
    let update_start_us = tracer.elapsed_us();
    let (preparation, draw_start_us, draw_end_us) = draw_frame(terminal, workbench, tracer)?;
    tracer.record(FrameRecord::new(
        Vec::new(),
        generation,
        workbench.diff_model(),
        &preparation,
        scroll,
        update_start_us,
        None,
        draw_start_us,
        draw_end_us,
    ));
    while !workbench.should_quit() && !shutdown.load(Ordering::Relaxed) {
        workbench.tick();
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
        let input_time = Instant::now();
        events.retain(|event| wheel_friction.accepts(event, input_time));
        let scroll_before = (
            workbench.diff_model().diff_scroll,
            workbench.diff_model().diff_horizontal_scroll,
        );
        let update_start_us = tracer.elapsed_us();
        drain_repository_events(repository_service, workbench, &mut generation);
        tool_tasks.drain(workbench);
        dispatch_events(&events, terminal, workbench, repository_service)?;
        let (preparation, draw_start_us, draw_end_us) = draw_frame(terminal, workbench, tracer)?;
        tracer.record(FrameRecord::new(
            input_events,
            generation,
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
        let _ = repository_service.cancel_command(command_id);
    }

    Ok(())
}

fn trace_input_events(events: &[Event], redact: bool) -> Vec<String> {
    if redact {
        return events
            .iter()
            .map(|_| "GitPrompt([redacted])".to_owned())
            .collect();
    }
    events.iter().map(|event| format!("{event:?}")).collect()
}

fn draw_frame(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    workbench: &mut Workbench,
    tracer: &FrameTracer,
) -> Result<(diffo_tui::FramePreparation, u64, u64)> {
    let size = terminal.size()?;
    let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
    let preparation = workbench.prepare_frame(area);
    let draw_start_us = tracer.elapsed_us();
    terminal.draw(|frame| workbench.render(frame))?;
    let draw_end_us = tracer.elapsed_us();
    Ok((preparation, draw_start_us, draw_end_us))
}

fn dispatch_events(
    events: &[crossterm::event::Event],
    terminal: &Terminal<CrosstermBackend<io::Stdout>>,
    workbench: &mut Workbench,
    repository_service: &RepositoryService,
) -> Result<()> {
    let size = terminal.size()?;
    let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
    for effect in workbench.handle_events(events, area) {
        dispatch_effect(effect, workbench, repository_service);
    }
    while let Some(command) = workbench.take_repository_command() {
        let id = command.id;
        let action = command.action;
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
                    workbench.show_toast(ToastKind::Error, format!("Could not copy path: {error}"));
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

fn drain_repository_events(
    repository_service: &RepositoryService,
    workbench: &mut Workbench,
    generation: &mut u64,
) {
    while let Ok(Some(event)) = repository_service.try_recv() {
        match event {
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
            RepositoryEvent::SnapshotRefreshed {
                generation: next,
                snapshot,
            } if next > *generation => {
                *generation = next;
                workbench.repository_changed(snapshot);
            }
            RepositoryEvent::RefreshFailed {
                generation: next,
                message,
            } if next > *generation => {
                *generation = next;
                workbench.operation_failed(message);
            }
            RepositoryEvent::CommandCompleted {
                generation: next,
                command_id,
                action,
                result,
                snapshot,
            } if next > *generation => {
                *generation = next;
                workbench.operation_completed(command_id, action, result, snapshot);
            }
            RepositoryEvent::CommandFailed {
                generation: next,
                command_id,
                failure,
            } if next > *generation => {
                *generation = next;
                workbench.action_failed(command_id, failure);
            }
            RepositoryEvent::CommandCancelled {
                generation: next,
                command_id,
                action,
            } if next > *generation => {
                *generation = next;
                workbench.operation_cancelled(command_id, action);
            }
            RepositoryEvent::SnapshotRefreshed { .. }
            | RepositoryEvent::RefreshFailed { .. }
            | RepositoryEvent::CommandCompleted { .. }
            | RepositoryEvent::CommandFailed { .. }
            | RepositoryEvent::CommandCancelled { .. } => {}
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
    use std::time::{Duration, Instant};

    use crossterm::{
        Command,
        event::{Event, KeyModifiers, MouseEvent, MouseEventKind},
    };

    use super::{EnableActionMouseCapture, WheelFriction, trace_input_events};

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
    fn wheel_friction_preserves_active_scroll_and_cuts_off_the_tail() {
        let mut friction = WheelFriction::default();
        let started = Instant::now();
        let down = wheel(MouseEventKind::ScrollDown);

        assert!(friction.accepts(&down, started));
        assert!(friction.accepts(&down, started));
        assert!(friction.accepts(&down, started));
        assert!(friction.accepts(&down, started + Duration::from_millis(48)));
        assert!(!friction.accepts(&down, started + Duration::from_millis(97)));
        assert!(friction.accepts(&down, started + Duration::from_millis(217)));
        assert!(friction.accepts(
            &wheel(MouseEventKind::ScrollUp),
            started + Duration::from_millis(218)
        ));
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

    fn wheel(kind: MouseEventKind) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            column: 50,
            row: 10,
            modifiers: KeyModifiers::NONE,
        })
    }
}
