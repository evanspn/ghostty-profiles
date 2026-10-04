//! The terminal UI: tabs for Profiles, Themes, Edit and Shaders.

pub mod app;
pub mod color;
pub mod fields;
pub mod ui;

#[cfg(test)]
mod tests;

use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use ratatui::crossterm::execute;

use crate::ghostty::Reloader;
use crate::store::Store;
pub use app::App;

/// Run the TUI until the user quits.
pub fn run(store: Store, reloader: Box<dyn Reloader>) -> Result<()> {
    let mut app = App::new(store, reloader)?;
    app.truecolor = color::detect_truecolor();
    let mut terminal = ratatui::init();
    // mouse support for the color picker; always handed back, on exit, error and panic
    let _ = execute!(std::io::stdout(), EnableMouseCapture);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
        previous(info);
    }));
    let result = event_loop(&mut terminal, &mut app);
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    while !app.quit {
        terminal.draw(|f| ui::draw(f, app))?;
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key),
                Event::Mouse(m) => app.on_mouse(m),
                _ => {}
            }
        }
        app.tick(Instant::now());
    }
    Ok(())
}
