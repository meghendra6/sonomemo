pub(crate) mod editing;
pub(crate) mod navigate;
pub(crate) mod popups;
pub(crate) mod search;

use crate::{app::App, models::InputMode};
use crossterm::event::{self, Event, KeyEventKind};

pub fn handle_event(app: &mut App, event: Event) {
    match event {
        Event::Mouse(mouse_event) => match mouse_event.kind {
            event::MouseEventKind::ScrollUp => app.scroll_up(),
            event::MouseEventKind::ScrollDown => app.scroll_down(),
            _ => {}
        },
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            if popups::handle_popup_events(app, key) {
                return;
            }
            match app.input_mode {
                InputMode::Navigate => navigate::handle_normal_mode(app, key),
                InputMode::Editing => editing::handle_editing_mode(app, key),
                InputMode::Search => search::handle_search_mode(app, key),
            }
        }
        _ => {}
    }
}
