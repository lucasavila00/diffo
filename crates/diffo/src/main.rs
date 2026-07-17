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
use diffo_core::{RepositorySource, fixture_source::FixtureRepositorySource};
use diffo_git::GitRepositorySource;
use diffo_tui::App;
use ratatui::{Terminal, backend::CrosstermBackend};

fn main() -> Result<()> {
    let shutdown = install_signal_handlers()?;
    let source: Box<dyn RepositorySource> = match env::var_os("DIFFO_MOCK_FILE") {
        Some(path) => Box::new(FixtureRepositorySource::new(path)),
        None => Box::new(GitRepositorySource::default()),
    };
    let snapshot = source.snapshot()?;
    if let Some(path) = env::var_os("DIFFO_DUMP_PATH") {
        return dump_snapshot(Path::new(&path), &snapshot);
    }

    let mut app = App::new(snapshot);
    let mut terminal = ratatui::init();

    let result = run(&mut terminal, &mut app, &shutdown);
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
                KeyCode::Down | KeyCode::Char('j') => app.scroll_down(),
                KeyCode::Up | KeyCode::Char('k') => app.scroll_up(),
                KeyCode::PageDown => app.page_down(),
                KeyCode::PageUp => app.page_up(),
                KeyCode::Home | KeyCode::Char('g') => app.scroll_to_top(),
                KeyCode::End | KeyCode::Char('G') => app.scroll_to_bottom(),
                _ => {}
            }
        }
    }

    Ok(())
}
