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
};
use diffo_core::{Repository, fixture_source::FixtureRepositorySource};
use diffo_git::GitRepositorySource;
use diffo_tui::{App, Effect};
use ratatui::{Terminal, backend::CrosstermBackend};

fn main() -> Result<()> {
    let shutdown = install_signal_handlers()?;
    let repository: Box<dyn Repository> = if let Some(path) = env::var_os("DIFFO_MOCK_FILE") {
        Box::new(FixtureRepositorySource::new(path))
    } else {
        Box::new(GitRepositorySource::default())
    };
    let snapshot = repository.snapshot()?;
    if let Some(path) = env::var_os("DIFFO_DUMP_PATH") {
        return dump_snapshot(Path::new(&path), &snapshot);
    }

    let mut app = App::new(snapshot, repository.access_mode());
    let mut terminal = ratatui::init();
    execute!(terminal.backend_mut(), EnableMouseCapture)?;

    let result = run(&mut terminal, &mut app, &shutdown, repository.as_ref());
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
    app: &mut App,
    shutdown: &AtomicBool,
    repository: &dyn Repository,
) -> Result<()> {
    while !app.should_quit && !shutdown.load(Ordering::Relaxed) {
        terminal.draw(|frame| diffo_tui::render(frame, app))?;

        if event::poll(Duration::from_millis(250))?
            && let Some(action) = diffo_tui::map_event(&event::read()?)
        {
            let size = terminal.size()?;
            let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
            if let Some(effect) = diffo_tui::dispatch(app, action, area) {
                execute_effect(repository, app, effect);
            }
        }
    }

    Ok(())
}

fn execute_effect(repository: &dyn Repository, app: &mut App, effect: Effect) {
    let Effect::Repository(action) = effect;
    if let Err(error) = repository
        .apply(&action)
        .and_then(|()| repository.snapshot())
        .map(|snapshot| app.refresh(snapshot))
    {
        app.show_error(error.to_string());
    }
}
