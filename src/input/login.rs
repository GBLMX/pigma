use crate::{
    app::App,
    event::{AuthEvent, NavigationEvent},
    key::{KeyCode, KeyPress},
    state::{LoginMethod, Page},
};

pub(super) fn handle_login_key(app: &mut App, key_event: KeyPress) -> bool {
    // Ctrl+C and Ctrl+P are handled globally in input.rs
    match key_event.code {
        KeyCode::Esc => app.state.events.send(NavigationEvent::Navigate(Page::Main)),
        // `Tab` walks the inputs of the method on screen; the method itself is switched with
        // the arrows, which is the one pair of keys the QR tab can spare.
        KeyCode::Tab => app.state.login.focus_field(1),
        KeyCode::BackTab => app.state.login.focus_field(-1),
        KeyCode::Left => app.state.login.select_method(-1),
        KeyCode::Right => app.state.login.select_method(1),
        KeyCode::Enter => submit(app),
        KeyCode::Backspace => {
            if let Some(input) = app.state.login.focused_input_mut() {
                input.delete_char();
            }
        }
        KeyCode::Char(c) if !key_event.mods.ctrl => {
            if let Some(input) = app.state.login.focused_input_mut() {
                input.enter_char(c);
            }
        }
        _ => {}
    }
    true
}

/// `Enter`: every method's single action. What it does to the form is the app's business —
/// this only says which method was asked for.
fn submit(app: &mut App) {
    if app.state.login.loading {
        return;
    }
    match app.state.login.method {
        // The QR tab keeps its own event: it is the one method that stays busy for minutes
        // rather than for one request.
        LoginMethod::Qr => app.state.events.send(AuthEvent::Login),
        method => app.state.events.send(AuthEvent::Submit(method)),
    }
}
