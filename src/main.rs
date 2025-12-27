//! Main entrypoint: terminal lifecycle, run loop, UI draw, and delegation.

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend},
};
use std::{error::Error, io};

mod actions;
mod app;
mod config;
mod editor;
mod input;
mod models;
mod runtime;
mod storage;
mod ui;

use app::App;
use models::InputMode;

fn main() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();

    // Initialize terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture,)?;

    // Keyboard enhancement flags may fail on unsupported terminals (e.g., Windows Legacy Console).
    // Errors are ignored as they don't affect app functionality.
    let _ = execute!(
        stdout,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);

    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        runtime::tick(app);

        terminal.draw(|f| ui::ui(f, app))?;

        // Block all input during pomodoro completion alert (forces break/attention)
        if app.pomodoro_alert_expiry.is_some() {
            if event::poll(std::time::Duration::from_millis(100))? {
                let _ = event::read();
            }
            continue;
        }

        if event::poll(std::time::Duration::from_millis(250))? {
            let event = event::read()?;

            if let Event::Mouse(mouse_event) = event {
                match mouse_event.kind {
                    event::MouseEventKind::ScrollUp => app.scroll_up(),
                    event::MouseEventKind::ScrollDown => app.scroll_down(),
                    _ => {}
                }
            }

            if let Event::Key(key) = event
                && key.kind == KeyEventKind::Press
            {
                handle_key_input(app, key);
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn handle_key_input(app: &mut App, key: event::KeyEvent) {
    if input::popups::handle_popup_events(app, key) {
        return;
    }

    match app.input_mode {
        InputMode::Navigate => input::navigate::handle_normal_mode(app, key),
        InputMode::Editing => input::editing::handle_editing_mode(app, key),
        InputMode::Search => input::search::handle_search_mode(app, key),
    }
}
