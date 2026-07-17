use std::{env, io, time::Duration};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use git_diff_tui::{
    app::App, fixture_source::FixtureRepositorySource, git_source::GitRepositorySource,
    repository::RepositorySource, ui,
};
use ratatui::{Terminal, backend::CrosstermBackend};

fn main() -> Result<()> {
    let source: Box<dyn RepositorySource> = match env::var_os("DIFFO_MOCK_FILE") {
        Some(path) => Box::new(FixtureRepositorySource::new(path)),
        None => Box::new(GitRepositorySource),
    };
    let mut app = App::new(source.snapshot()?);
    let mut terminal = ratatui::init();

    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| ui::render(frame, app))?;

        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
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
