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
use ratatui::{Terminal, backend::CrosstermBackend};

fn main() -> Result<()> {
    let shutdown = install_signal_handlers()?;
    let repository: Box<dyn Repository> = if let Some(path) = env::var_os("DIFFO_MOCK_FILE") {
        if env::var_os("DIFFO_MOCK_MUTABLE").is_some() {
            Box::new(MutableFixtureRepository::new_with_large_files(path)?)
        } else {
            Box::new(FixtureRepositorySource::new(path))
        }
    } else {
        Box::new(GitRepositorySource::default())
    };
    let snapshot = repository.snapshot()?;
    if let Some(path) = env::var_os("DIFFO_DUMP_PATH") {
        return dump_snapshot(Path::new(&path), &snapshot);
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

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    renderer: &mut diffo_tui::Renderer,
    model: &mut Model,
    shutdown: &AtomicBool,
    repository: &dyn Repository,
) -> Result<()> {
    while !model.should_quit && !shutdown.load(Ordering::Relaxed) {
        terminal.draw(|frame| renderer.render(frame, model))?;

        let poll_timeout = if renderer.is_preparing() {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(250)
        };
        if event::poll(poll_timeout)? {
            let size = terminal.size()?;
            let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
            if let Some(message) = renderer.map_event(&event::read()?, model, area)
                && let Some(effect) = update(model, message)
            {
                execute_effect(repository, model, effect);
            }
        }
    }

    Ok(())
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
