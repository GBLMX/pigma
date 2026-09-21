use crossterm::event::{KeyCode, KeyEvent, MouseEventKind};

use crate::app::App;

/// The keys of whichever popup is up.
///
/// One entry point for all of them: they are the same list with a title and a footer, so the keys
/// that walk one walk all three, and a popup that is up is the only thing the keys are for.
pub(super) fn handle_popup_key(app: &mut App, key_event: KeyEvent) -> bool {
    if app.state.tasks_popup.open {
        return walk(&mut app.state.tasks_popup, key_event) || clear_finished(app, key_event);
    }
    if app.state.messages.open {
        return walk(&mut app.state.messages, key_event);
    }
    if app.state.help.open {
        return walk(&mut app.state.help, key_event);
    }

    false
}

/// The keys every popup shares: close, and walk the list.
fn walk(popup: &mut crate::state::popup::PopupState, key_event: KeyEvent) -> bool {
    match key_event.code {
        KeyCode::Esc | KeyCode::Char('q') => popup.close(),
        KeyCode::Up | KeyCode::Char('k' | 'K') => popup.scroll_up(),
        KeyCode::Down | KeyCode::Char('j' | 'J') => popup.scroll_down(),
        KeyCode::Char('g') => popup.scroll_top(),
        KeyCode::Char('G') => popup.scroll_bottom(),
        _ => return false,
    }

    true
}

/// `x` on the task list forgets what is over, which is what a reader who has read it wants next.
fn clear_finished(app: &mut App, key_event: KeyEvent) -> bool {
    if key_event.code != KeyCode::Char('x') {
        return false;
    }
    app.state.tasks.clear_finished();
    app.state.tasks_popup.max_scroll = 0;

    true
}

pub(super) fn handle_help_mouse(app: &mut App, kind: MouseEventKind) {
    match kind {
        MouseEventKind::ScrollUp => app.state.help.scroll_up(),
        MouseEventKind::ScrollDown => app.state.help.scroll_down(),
        _ => {}
    }
}
