//! Keyboard (and mouse) input handling: dispatches key events to the per-view
//! handlers (navigation, content, search, login, help, command, splash, table).

mod command;
mod content;
pub(crate) mod ex;
mod help;
mod hit;
mod login;
mod main;
mod navigation;
pub(crate) mod pages;
mod panes;
mod search;
mod splash;
mod table;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};

use crate::{
    app::App,
    event::{AppEvent, CommandEvent, CommandPanelAction, NavigationEvent},
    state::Page,
};

/// Insert a pasted block into whichever single-line editor has focus.
///
/// Every editor in the UI is a [`TextInput`], and only one of them can have focus, so
/// this mirrors the key dispatch: the command line owns input while it is open,
/// otherwise the search box. Text goes in at the cursor, and control characters are
/// dropped — a pasted document must not turn its own newlines into Enter presses.
pub fn handle_paste(app: &mut App, text: &str) {
    let input = if app.state.prompt.active {
        Some(&mut app.state.prompt.input)
    } else if app.state.navigation.search.active {
        Some(&mut app.state.navigation.search.input)
    } else {
        None
    };

    let Some(input) = input else {
        return;
    };

    for ch in text.chars().filter(|c| !c.is_control()) {
        input.enter_char(ch);
    }
}

pub fn handle_key_events(app: &mut App, key_event: KeyEvent) -> color_eyre::Result<()> {
    if key_event.modifiers == KeyModifiers::CONTROL {
        match key_event.code {
            KeyCode::Char('c' | 'C') => {
                app.state.events.send(AppEvent::Quit);
                return Ok(());
            }
            KeyCode::Char('p' | 'P') => {
                app.state
                    .events
                    .send(CommandEvent::Panel(CommandPanelAction::Open));
                return Ok(());
            }
            _ => {}
        }
    }

    // The prompt owns every key while it is open, including the global shortcuts.
    if app.state.prompt.active && ex::handle_ex_key(app, key_event) {
        return Ok(());
    }

    if key_event.code == KeyCode::Char('?') {
        app.state.help.toggle();
        return Ok(());
    }

    if app.state.navigation.page == Page::Splash {
        splash::handle_splash_key(app, key_event);
        return Ok(());
    }

    if help::handle_popup_key(app, key_event) {
        return Ok(());
    }

    if app.state.command_panel.open {
        command::handle_command_key(app, key_event);
        return Ok(());
    }

    if app.state.navigation.page == Page::Login {
        login::handle_login_key(app, key_event);
        return Ok(());
    }

    if app.state.navigation.search.active && search::handle_search_key(app, key_event) {
        return Ok(());
    }

    // Vim-style command line; `:` is Shift+; on most layouts, so no modifier check.
    if key_event.code == KeyCode::Char(':') {
        ex::open(app);
        return Ok(());
    }

    // Uppercase L: go to the login page (lowercase `l` toggles lyrics on the main page).
    // No-op when already logged in.
    if key_event.code == KeyCode::Char('L') && !app.service.client().is_logged_in() {
        app.state
            .events
            .send(NavigationEvent::Navigate(Page::Login));
        return Ok(());
    }

    main::handle_main_key(app, key_event)
}

pub fn handle_mouse_event(app: &mut App, kind: MouseEventKind, col: u16, row: u16) {
    if app.state.help.open {
        help::handle_help_mouse(app, kind);
        return;
    }

    if app.state.command_panel.open {
        command::handle_command_mouse(app, kind);
        return;
    }

    main::handle_main_mouse(app, kind, col, row);
}
