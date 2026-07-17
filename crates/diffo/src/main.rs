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
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use diffo_core::{Repository, RepositoryAction, fixture_source::FixtureRepositorySource};
use diffo_git::GitRepositorySource;
use diffo_tui::App;
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

    let result = run(&mut terminal, &mut app, &shutdown, repository.as_ref());
    ratatui::restore();
    result
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
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.should_quit = true;
                }
                KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
                KeyCode::Home | KeyCode::Char('g') => app.select_first(),
                KeyCode::End | KeyCode::Char('G') => app.select_last(),
                KeyCode::Char('s') => apply_action(repository, app, app.stage_selected()),
                KeyCode::Char('u') => apply_action(repository, app, app.unstage_selected()),
                KeyCode::Char('a') => apply_action(repository, app, app.stage_all()),
                _ => {}
            }
        }
    }

    Ok(())
}

fn apply_action(repository: &dyn Repository, app: &mut App, action: Option<RepositoryAction>) {
    let Some(action) = action else {
        return;
    };
    if let Err(error) = repository
        .apply(&action)
        .and_then(|()| repository.snapshot())
        .map(|snapshot| app.refresh(snapshot))
    {
        app.show_error(error.to_string());
    }
}
