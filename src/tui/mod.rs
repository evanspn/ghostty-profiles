//! The terminal UI: tabs for Profiles, Themes, Edit and Shaders.

pub mod app;
pub mod fields;
pub mod ui;

#[cfg(test)]
mod tests;

use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use crate::ghostty::Reloader;
use crate::store::Store;
pub use app::App;

/// Run the TUI until the user quits.
pub fn run(store: Store, reloader: Box<dyn Reloader>) -> Result<()> {
    let mut app = App::new(store, reloader)?;
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    while !app.quit {
        terminal.draw(|f| ui::draw(f, app))?;
        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.on_key(key);
        }
        app.tick(Instant::now());
    }
    Ok(())
}
