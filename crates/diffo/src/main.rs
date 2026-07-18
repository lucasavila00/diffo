use std::{
    env, fmt, fs,
    io::{self, Write as _},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
#[cfg(windows)]
use crossterm::event::EnableMouseCapture;
use crossterm::{
    Command,
    event::{self, DisableMouseCapture},
    execute,
    terminal::{Clear, ClearType},
};
use diffo_app::ToastKind;
use diffo_core::{Repository, fixture_source::MutableFixtureRepository};
use diffo_git::GitRepositorySource;
use diffo_watch::{RefreshResult, RefreshService};
use ratatui::{Terminal, backend::CrosstermBackend};

mod frame_trace;
mod tool_tasks;

use diffo_workbench::{Workbench, WorkbenchEffect};
use frame_trace::{FrameRecord, FrameTracer};
use tool_tasks::ToolTasks;

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
    let refresh = watch_paths
        .as_deref()
        .map(|paths| RefreshService::start(Arc::clone(&repository), paths))
        .transpose()?;
    if let Some(path) = env::var_os("DIFFO_WATCH_DUMP_PATH") {
        return run_watch_dump(
            Path::new(&path),
            &snapshot,
            refresh
                .as_ref()
                .context("watch dump requires a real Git repository")?,
            &shutdown,
        );
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
        repository.as_ref(),
        refresh.as_ref(),
        &tool_tasks,
        &mut tracer,
    );
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
    refresh: &RefreshService,
    shutdown: &AtomicBool,
) -> Result<()> {
    dump_snapshot_atomic(path, initial)?;
    let mut generation = 0;
    while !shutdown.load(Ordering::Relaxed) {
        while let Ok(Some(result)) = refresh.try_recv() {
            match result {
                RefreshResult::Snapshot {
                    generation: next,
                    snapshot,
                } if next > generation => {
                    generation = next;
                    dump_snapshot_atomic(path, &snapshot)?;
                }
                RefreshResult::Error { message, .. } => eprintln!("{message}"),
                RefreshResult::Snapshot { .. }
                | RefreshResult::ActionCompleted { .. }
                | RefreshResult::ActionFailed { .. } => {}
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
    repository: &dyn Repository,
    refresh: Option<&RefreshService>,
    tool_tasks: &ToolTasks,
    tracer: &mut FrameTracer,
) -> Result<()> {
    let mut generation = 0;
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
            || refresh.is_some_and(RefreshService::is_busy)
            || workbench.diff_model().network_operation().is_some()
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
        let scroll_before = (
            workbench.diff_model().diff_scroll,
            workbench.diff_model().diff_horizontal_scroll,
        );
        let update_start_us = tracer.elapsed_us();
        if let Some(refresh) = refresh {
            drain_refresh(refresh, workbench, &mut generation);
        }
        tool_tasks.drain(workbench);
        dispatch_events(&events, terminal, workbench, repository, refresh)?;
        let input_events = events.iter().map(|event| format!("{event:?}")).collect();
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

    Ok(())
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
    repository: &dyn Repository,
    refresh: Option<&RefreshService>,
) -> Result<()> {
    let size = terminal.size()?;
    let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
    for effect in workbench.handle_events(events, area) {
        dispatch_effect(effect, workbench, repository, refresh);
    }
    Ok(())
}

fn dispatch_effect(
    effect: WorkbenchEffect,
    workbench: &mut Workbench,
    repository: &dyn Repository,
    refresh: Option<&RefreshService>,
) {
    match effect {
        WorkbenchEffect::Repository(action) if refresh.is_some() => {
            refresh.expect("checked above").apply(action);
        }
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
        WorkbenchEffect::Repository(action) => execute_effect(repository, workbench, action),
    }
}

fn drain_refresh(refresh: &RefreshService, workbench: &mut Workbench, generation: &mut u64) {
    while let Ok(Some(result)) = refresh.try_recv() {
        match result {
            RefreshResult::Snapshot {
                generation: next,
                snapshot,
            } if next > *generation => {
                *generation = next;
                workbench.repository_changed(snapshot);
            }
            RefreshResult::Error {
                generation: next,
                message,
            } if next > *generation => {
                *generation = next;
                workbench.operation_failed(message);
            }
            RefreshResult::ActionCompleted {
                generation: next,
                action,
                result,
                snapshot,
            } if next > *generation => {
                *generation = next;
                workbench.operation_completed(action, result, snapshot);
            }
            RefreshResult::ActionFailed {
                generation: next,
                failure,
            } if next > *generation => {
                *generation = next;
                workbench.action_failed(failure);
            }
            RefreshResult::Snapshot { .. }
            | RefreshResult::Error { .. }
            | RefreshResult::ActionCompleted { .. }
            | RefreshResult::ActionFailed { .. } => {}
        }
    }
}

fn execute_effect(
    repository: &dyn Repository,
    workbench: &mut Workbench,
    action: diffo_core::RepositoryAction,
) {
    match repository.apply(&action) {
        Ok(result) => match repository.snapshot() {
            Ok(snapshot) => workbench.operation_completed(action, result, snapshot),
            Err(error) => workbench.operation_failed(error.to_string()),
        },
        Err(failure) => workbench.action_failed(failure),
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
    use crossterm::Command;

    use super::EnableActionMouseCapture;

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
}
