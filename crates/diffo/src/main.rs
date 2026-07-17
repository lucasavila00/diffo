use std::{
    env, fs, io,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{Clear, ClearType},
};
use diffo_app::{Effect, Message, Model, update};
use diffo_core::{
    Repository,
    fixture_source::{FixtureRepositorySource, MutableFixtureRepository},
};
use diffo_git::GitRepositorySource;
use diffo_watch::{RefreshResult, RefreshService};
use ratatui::{Terminal, backend::CrosstermBackend};

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
                RefreshResult::Snapshot { .. } => {}
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
) -> Result<()> {
    let mut generation = 0;
    while !model.should_quit && !shutdown.load(Ordering::Relaxed) {
        if let Some(refresh) = refresh {
            drain_refresh(refresh, model, &mut generation);
        }
        terminal.draw(|frame| renderer.render(frame, model))?;

        let poll_timeout =
            if renderer.is_preparing() || refresh.is_some_and(RefreshService::is_busy) {
                Duration::from_millis(16)
            } else {
                Duration::from_millis(50)
            };
        if event::poll(poll_timeout)? {
            let size = terminal.size()?;
            let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
            if let Some(message) = renderer.map_event(&event::read()?, model, area)
                && let Some(effect) = update(model, message)
            {
                if let Some(refresh) = refresh {
                    let Effect::Repository(action) = effect;
                    refresh.apply(action);
                } else {
                    execute_effect(repository, model, effect);
                }
            }
        }
    }

    Ok(())
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
            RefreshResult::Snapshot { .. } | RefreshResult::Error { .. } => None,
        };
        if let Some(message) = message {
            let _ = update(model, message);
        }
    }
}

fn execute_effect(repository: &dyn Repository, model: &mut Model, effect: Effect) {
    let Effect::Repository(action) = effect;
    let message = match repository
        .apply(&action)
        .and_then(|()| repository.snapshot())
    {
        Ok(snapshot) => Message::SnapshotLoaded(snapshot),
        Err(error) => Message::OperationFailed(error.to_string()),
    };
    let _ = update(model, message);
}
