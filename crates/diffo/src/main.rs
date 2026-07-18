use std::{
    collections::HashMap,
    env, fs,
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
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{Clear, ClearType},
};
use diffo_app::{Effect, Message, Model, ToastKind, update};
use diffo_core::{
    Repository,
    fixture_source::{FixtureRepositorySource, MutableFixtureRepository},
};
use diffo_git::GitRepositorySource;
use diffo_watch::{RefreshResult, RefreshService};
use ratatui::{Terminal, backend::CrosstermBackend};

mod frame_trace;

use frame_trace::{FrameRecord, FrameTracer};

fn main() -> Result<()> {
    let shutdown = install_signal_handlers()?;
    let (repository, watch_paths): (Arc<dyn Repository>, Option<Vec<_>>) =
        if let Some(path) = env::var_os("DIFFO_MOCK_FILE") {
            if env::var_os("DIFFO_MOCK_MUTABLE").is_some() {
                (
                    Arc::new(MutableFixtureRepository::new_with_large_files(path)?),
                    None,
                )
            } else {
                (Arc::new(FixtureRepositorySource::new(path)), None)
            }
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

    let mut model = Model::new(snapshot, repository.access_mode());
    let mut renderer = diffo_tui::Renderer::new();
    let mut tracer = FrameTracer::from_environment();
    let mut terminal = ratatui::init();
    execute!(
        terminal.backend_mut(),
        Clear(ClearType::Purge),
        EnableMouseCapture
    )?;

    let result = run(
        &mut terminal,
        &mut renderer,
        &mut model,
        &shutdown,
        repository.as_ref(),
        refresh.as_ref(),
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
    renderer: &mut diffo_tui::Renderer,
    model: &mut Model,
    shutdown: &AtomicBool,
    repository: &dyn Repository,
    refresh: Option<&RefreshService>,
    tracer: &mut FrameTracer,
) -> Result<()> {
    let mut generation = 0;
    let mut toast_deadlines = HashMap::new();
    let scroll = (model.diff_scroll, model.diff_horizontal_scroll);
    let update_start_us = tracer.elapsed_us();
    let (preparation, draw_start_us, draw_end_us) = draw_frame(terminal, renderer, model, tracer)?;
    tracer.record(FrameRecord::new(
        Vec::new(),
        generation,
        model,
        preparation,
        scroll,
        update_start_us,
        None,
        draw_start_us,
        draw_end_us,
    ));
    while !model.should_quit && !shutdown.load(Ordering::Relaxed) {
        expire_toasts(model, &mut toast_deadlines);
        let poll_timeout = if renderer.is_preparing()
            || refresh.is_some_and(RefreshService::is_busy)
            || model.network_operation().is_some()
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
        let scroll_before = (model.diff_scroll, model.diff_horizontal_scroll);
        let update_start_us = tracer.elapsed_us();
        if let Some(refresh) = refresh {
            drain_refresh(refresh, model, &mut generation);
        }
        dispatch_events(&events, terminal, renderer, model, repository, refresh)?;
        let input_events = events.iter().map(|event| format!("{event:?}")).collect();
        let (preparation, draw_start_us, draw_end_us) =
            draw_frame(terminal, renderer, model, tracer)?;
        tracer.record(FrameRecord::new(
            input_events,
            generation,
            model,
            preparation,
            scroll_before,
            update_start_us,
            event_read_us,
            draw_start_us,
            draw_end_us,
        ));
    }

    Ok(())
}

fn expire_toasts(model: &mut Model, deadlines: &mut HashMap<u64, Instant>) {
    let now = Instant::now();
    deadlines.retain(|id, _| model.toasts.iter().any(|toast| toast.id == *id));
    for toast in &model.toasts {
        if toast.kind != ToastKind::Error {
            deadlines
                .entry(toast.id)
                .or_insert_with(|| now + Duration::from_secs(3));
        }
    }
    let expired = deadlines
        .iter()
        .filter_map(|(id, deadline)| (*deadline <= now).then_some(*id))
        .collect::<Vec<_>>();
    for id in expired {
        let _ = update(model, Message::DismissToast(id));
        deadlines.remove(&id);
    }
}

fn draw_frame(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    renderer: &mut diffo_tui::Renderer,
    model: &mut Model,
    tracer: &FrameTracer,
) -> Result<(diffo_tui::FramePreparation, u64, u64)> {
    let size = terminal.size()?;
    let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
    let preparation = renderer.prepare_frame(model, area);
    if let Some(row) = preparation.anchored_vertical_scroll {
        model.anchor_diff_scroll(row);
    }
    model.clamp_diff_scroll(
        preparation.maximum_vertical_scroll,
        preparation.maximum_horizontal_scroll,
    );
    let draw_start_us = tracer.elapsed_us();
    terminal.draw(|frame| renderer.render(frame, model))?;
    let draw_end_us = tracer.elapsed_us();
    Ok((preparation, draw_start_us, draw_end_us))
}

fn dispatch_events(
    events: &[crossterm::event::Event],
    terminal: &Terminal<CrosstermBackend<io::Stdout>>,
    renderer: &mut diffo_tui::Renderer,
    model: &mut Model,
    repository: &dyn Repository,
    refresh: Option<&RefreshService>,
) -> Result<()> {
    let size = terminal.size()?;
    let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
    let mut scroll = PendingScroll::default();
    for event in events {
        let Some(message) = renderer.map_event(event, model, area) else {
            continue;
        };
        if !scroll.push(&message) {
            scroll.flush(model);
            dispatch_message(message, model, repository, refresh);
        }
    }
    scroll.flush(model);
    Ok(())
}

fn dispatch_message(
    message: Message,
    model: &mut Model,
    repository: &dyn Repository,
    refresh: Option<&RefreshService>,
) {
    if let Some(effect) = update(model, message) {
        match effect {
            Effect::Repository(action) if refresh.is_some() => {
                refresh.expect("checked above").apply(action);
            }
            Effect::CopyPath { path, absolute } => match copy_path_to_clipboard(&path, absolute) {
                Ok(copied) => {
                    model.show_toast(
                        ToastKind::Success,
                        format!(
                            "Copied {} path: {copied}",
                            if absolute { "absolute" } else { "relative" }
                        ),
                    );
                }
                Err(error) => {
                    model.show_toast(ToastKind::Error, format!("Could not copy path: {error}"));
                }
            },
            effect @ Effect::Repository(_) => execute_effect(repository, model, effect),
        }
    }
}

#[derive(Default)]
struct PendingScroll {
    vertical: i64,
    horizontal: i64,
}

impl PendingScroll {
    fn push(&mut self, message: &Message) -> bool {
        match message {
            Message::ScrollDiffUp => self.vertical = self.vertical.saturating_sub(4),
            Message::ScrollDiffDown => self.vertical = self.vertical.saturating_add(4),
            Message::ScrollDiffPageUp(lines) => {
                self.vertical = self
                    .vertical
                    .saturating_sub(i64::try_from(*lines).unwrap_or(i64::MAX));
            }
            Message::ScrollDiffPageDown(lines) => {
                self.vertical = self
                    .vertical
                    .saturating_add(i64::try_from(*lines).unwrap_or(i64::MAX));
            }
            Message::ScrollDiffBy(lines) => {
                self.vertical = self.vertical.saturating_add(*lines);
            }
            Message::ScrollDiffLeft => self.horizontal = self.horizontal.saturating_sub(4),
            Message::ScrollDiffRight => self.horizontal = self.horizontal.saturating_add(4),
            Message::ScrollDiffHorizontalBy(columns) => {
                self.horizontal = self.horizontal.saturating_add(*columns);
            }
            _ => return false,
        }
        true
    }

    fn flush(&mut self, model: &mut Model) {
        if self.vertical != 0 {
            let _ = update(model, Message::ScrollDiffBy(self.vertical));
        }
        if self.horizontal != 0 {
            let _ = update(model, Message::ScrollDiffHorizontalBy(self.horizontal));
        }
        *self = Self::default();
    }
}

fn drain_refresh(refresh: &RefreshService, model: &mut Model, generation: &mut u64) {
    while let Ok(Some(result)) = refresh.try_recv() {
        let message = match result {
            RefreshResult::Snapshot {
                generation: next,
                snapshot,
            } if next > *generation => {
                *generation = next;
                Some(Message::SnapshotLoaded(snapshot))
            }
            RefreshResult::Error {
                generation: next,
                message,
            } if next > *generation => {
                *generation = next;
                Some(Message::OperationFailed(message))
            }
            RefreshResult::ActionCompleted {
                generation: next,
                result,
                snapshot,
            } if next > *generation => {
                *generation = next;
                Some(Message::OperationCompleted(result, snapshot))
            }
            RefreshResult::ActionFailed {
                generation: next,
                failure,
            } if next > *generation => {
                *generation = next;
                Some(Message::ActionFailed(failure))
            }
            RefreshResult::Snapshot { .. }
            | RefreshResult::Error { .. }
            | RefreshResult::ActionCompleted { .. }
            | RefreshResult::ActionFailed { .. } => None,
        };
        if let Some(message) = message {
            let _ = update(model, message);
        }
    }
}

fn execute_effect(repository: &dyn Repository, model: &mut Model, effect: Effect) {
    let Effect::Repository(action) = effect else {
        return;
    };
    let message = match repository.apply(&action) {
        Ok(result) => match repository.snapshot() {
            Ok(snapshot) => Message::OperationCompleted(result, snapshot),
            Err(error) => Message::OperationFailed(error.to_string()),
        },
        Err(failure) => Message::ActionFailed(failure),
    };
    let _ = update(model, message);
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
    use diffo_app::{Message, Model};
    use diffo_core::{AccessMode, RepositorySnapshot};

    use super::PendingScroll;

    #[test]
    fn coalesces_ready_scroll_events_into_one_transition() {
        let mut pending = PendingScroll::default();
        for _ in 0..10 {
            assert!(pending.push(&Message::ScrollDiffDown));
        }
        let mut model = Model::new(RepositorySnapshot::default(), AccessMode::ReadWrite);

        pending.flush(&mut model);

        assert_eq!(model.diff_scroll, 40);
        assert_eq!(pending.vertical, 0);
    }

    #[test]
    fn coalesces_high_resolution_wheel_events() {
        let mut pending = PendingScroll::default();
        for _ in 0..10 {
            assert!(pending.push(&Message::ScrollDiffBy(1)));
        }
        let mut model = Model::new(RepositorySnapshot::default(), AccessMode::ReadWrite);

        pending.flush(&mut model);

        assert_eq!(model.diff_scroll, 10);
    }

    #[test]
    fn user_scroll_is_applied_after_refresh() {
        let mut model = Model::new(RepositorySnapshot::default(), AccessMode::ReadWrite);
        model.diff_scroll = 40;
        let mut pending = PendingScroll::default();
        assert!(pending.push(&Message::ScrollDiffDown));

        model.repository_changed(RepositorySnapshot::default());
        pending.flush(&mut model);

        assert_eq!(model.diff_scroll, 4);
    }
}
