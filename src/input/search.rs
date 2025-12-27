use crate::{actions, app::App, config::key_match, models::InputMode};
use crossterm::event::KeyEvent;

pub fn handle_search_mode(app: &mut App, key: KeyEvent) {
    if key_match(&key, &app.config.keybindings.search.cancel) {
        app.last_search_query = None;
        app.search_highlight_query = None;
        app.search_highlight_ready_at = None;
        app.transition_to(InputMode::Navigate);
    } else if key_match(&key, &app.config.keybindings.search.clear) {
        app.textarea = tui_textarea::TextArea::default();
        app.search_highlight_query = None;
        app.search_highlight_ready_at = None;
        app.transition_to(InputMode::Search);
    } else if key_match(&key, &app.config.keybindings.search.submit) {
        actions::submit_search(app);
        app.transition_to(InputMode::Navigate);
    } else {
        app.textarea.input(key);
    }
}
