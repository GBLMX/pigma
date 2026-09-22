use crate::{
    app::App,
    event::AppEvent,
    key::{KeyCode, KeyPress},
};

pub(super) fn handle_splash_key(app: &mut App, key_event: KeyPress) {
    match key_event.code {
        KeyCode::Esc | KeyCode::Char('q') => app.state.events.send(AppEvent::Quit),
        _ => {}
    }
}
