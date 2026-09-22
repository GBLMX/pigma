//! Dragging the panes: the mouse on an edge, and the keys that do the same thing.
//!
//! The frame itself is not draggable — the edges here are the ones *inside* it, between the
//! topbar, the navigation, the content and the player bar. Three gestures, deliberately the same
//! three Herdr offers for its splits:
//!
//! - **drag** an edge to resize the pane behind it (the sizes are clamped, see [`layout::clamp`]);
//! - **double click** an edge to collapse that pane, and again to put it back — the size is kept,
//!   so a pane that comes back comes back the way it was;
//! - **`Ctrl` + an arrow** moves the same edge the mouse would, two cells at a time.
//!
//! A drag writes the size into the config as it goes (the frame reads the config, so the pane
//! follows the mouse) and saves once, when the mouse is released: a drag is one decision.

use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEventKind};

use crate::{
    app::App,
    config::{NavPosition, Pane},
    key::{KeyCode, KeyPress},
    layout::{self, Axis},
    state::PaneDrag,
    utils::Named,
};

/// How long two clicks on the same edge count as a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// How far `Ctrl` + an arrow moves an edge.
const KEY_STEP: i32 = 2;

/// Handle a mouse event that may be about a pane edge. `true` when it was, so the caller leaves
/// the event alone — an edge is the frame's furniture, not something under it.
pub(super) fn handle_mouse(app: &mut App, kind: MouseEventKind, col: u16, row: u16) -> bool {
    match kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let Some(divider) = app.state.pane_dividers.at(col, row).copied() else {
                return false;
            };

            let double_clicked = app
                .state
                .last_pane_click
                .is_some_and(|(last, at)| last == divider && at.elapsed() < DOUBLE_CLICK);
            app.state.last_pane_click = Some((divider, Instant::now()));

            if double_clicked {
                let pane = divider.pane;
                app.config.panes.toggle(pane);
                app.state.pane_drag = None;
                app.config.save();
                app.toast(format!(
                    "{}: {}",
                    pane.describe(),
                    if app.config.panes.visible(pane) {
                        "开"
                    } else {
                        "关"
                    }
                ));
            } else {
                app.state.pane_drag = Some(PaneDrag {
                    divider,
                    start_size: app.config.panes.size(divider.pane),
                    col,
                    row,
                });
            }

            true
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            let Some(drag) = app.state.pane_drag else {
                return false;
            };

            let delta = match drag.divider.axis {
                Axis::Columns => i32::from(col) - i32::from(drag.col),
                Axis::Rows => i32::from(row) - i32::from(drag.row),
            };
            let wanted = drag.divider.size_for(drag.start_size, delta);
            resize(app, drag.divider.pane, drag.divider.axis, wanted);

            true
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let Some(drag) = app.state.pane_drag.take() else {
                return false;
            };

            app.config.save();
            app.toast(format!(
                "{}: {}",
                drag.divider.pane.describe(),
                size_label(drag.divider.pane, app.config.panes.size(drag.divider.pane))
            ));

            true
        }
        // A scroll over an edge belongs to whatever is under the mouse, not to the edge.
        _ => false,
    }
}

/// `Ctrl` + an arrow moves the edge a drag would: down/up move the topbar's and the player bar's
/// edges the way the mouse moves them, and left/right move the sidebar's.
pub(super) fn handle_key(app: &mut App, key: KeyPress) -> bool {
    if !key.mods.ctrl {
        return false;
    }

    let nav_is_column = matches!(
        app.config.navigation_position,
        NavPosition::Left | NavPosition::Right
    );
    let target = match key.code {
        // Down grows the topbar (its edge is dragged downwards), up grows the player bar.
        KeyCode::Down => Some((Pane::Topbar, Axis::Rows, 1)),
        KeyCode::Up => Some((Pane::Playerbar, Axis::Rows, 1)),
        KeyCode::Right if nav_is_column => Some((
            Pane::Navigation,
            Axis::Columns,
            if app.config.navigation_position == NavPosition::Left {
                1
            } else {
                -1
            },
        )),
        KeyCode::Left if nav_is_column => Some((
            Pane::Navigation,
            Axis::Columns,
            if app.config.navigation_position == NavPosition::Left {
                -1
            } else {
                1
            },
        )),
        _ => None,
    };

    let Some((pane, axis, sign)) = target else {
        return false;
    };

    let wanted = (i32::from(app.config.panes.size(pane)) + sign * KEY_STEP).max(0) as u16;
    resize(app, pane, axis, wanted);
    app.config.save();
    app.toast(format!(
        "{}: {}",
        pane.describe(),
        size_label(pane, app.config.panes.size(pane))
    ));

    true
}

/// Set a pane's size, clamped to what the frame can spare.
fn resize(app: &mut App, pane: Pane, axis: Axis, size: u16) {
    let available = match axis {
        Axis::Columns => app.state.shell_area.width,
        Axis::Rows => app.state.shell_area.height,
    };
    let clamped = layout::clamp(pane, axis, size, available);
    app.config.panes.set_size(pane, clamped);
}

/// How a size reads in a toast: the sidebar and the MV column are measured in columns, the rest
/// in rows.
fn size_label(pane: Pane, size: u16) -> String {
    match pane {
        Pane::Navigation | Pane::Mv => format!("{size} 列"),
        _ => format!("{size} 行"),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{
        config::{Config, PanesConfig},
        key::Modifiers,
    };

    /// An app on the main page, one frame drawn so the pane edges exist.
    fn app_with(panes: PanesConfig) -> App {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let config = Config {
            panes,
            ..Config::default()
        };
        let mut app = App::new(config, false).expect("app");
        app.state.navigation.page = crate::state::Page::Main;
        draw(&mut app);
        app
    }

    fn draw(app: &mut App) {
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("backend");
        terminal.draw(|f| crate::ui::draw(f, app)).expect("draw");
    }

    /// The player bar's top edge: a known row in a 120×40 frame.
    fn playerbar_edge(app: &App) -> (u16, u16) {
        let divider = app
            .state
            .pane_dividers
            .iter()
            .find(|divider| divider.pane == Pane::Playerbar)
            .expect("the player bar has an edge");
        (divider.rect.x, divider.rect.y)
    }

    /// Dragging the player bar's edge up makes the bar taller, and the size is saved once, when
    /// the mouse is released.
    #[tokio::test]
    async fn dragging_an_edge_resizes_the_pane() {
        let mut app = app_with(PanesConfig::default());
        let start = app.config.panes.playerbar;
        let (col, row) = playerbar_edge(&app);

        assert!(handle_mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            col + 5,
            row
        ));
        assert!(app.state.pane_drag.is_some(), "the drag did not start");

        handle_mouse(
            &mut app,
            MouseEventKind::Drag(MouseButton::Left),
            col + 5,
            row - 3,
        );
        assert_eq!(
            app.config.panes.playerbar,
            start + 3,
            "the bar did not follow the mouse"
        );

        handle_mouse(
            &mut app,
            MouseEventKind::Up(MouseButton::Left),
            col + 5,
            row - 3,
        );
        assert!(app.state.pane_drag.is_none(), "the drag did not end");
        draw(&mut app);
        assert_eq!(app.config.panes.playerbar, start + 3);
    }

    /// A drag is clamped: the player bar cannot be dragged past what the content needs, and the
    /// size that comes out is the clamped one — which is what a release saves.
    #[tokio::test]
    async fn a_drag_stops_at_the_clamp() {
        let mut app = app_with(PanesConfig::default());
        let (col, row) = playerbar_edge(&app);

        handle_mouse(&mut app, MouseEventKind::Down(MouseButton::Left), col, row);
        // Far above the top of the frame: the bar would be taller than the frame itself.
        handle_mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), col, 0);
        handle_mouse(&mut app, MouseEventKind::Up(MouseButton::Left), col, 0);

        assert_eq!(
            app.config.panes.playerbar,
            layout::clamp(Pane::Playerbar, Axis::Rows, u16::MAX, 40)
        );
        assert!(
            app.config.panes.playerbar < 40,
            "the bar took the whole frame"
        );
    }

    /// Two clicks on the same edge collapse the pane, and the size is kept for the way back.
    #[tokio::test]
    async fn a_double_click_collapses_and_restores() {
        let mut app = app_with(PanesConfig::default());
        let size = app.config.panes.playerbar;
        let (col, row) = playerbar_edge(&app);

        for _ in 0..2 {
            handle_mouse(&mut app, MouseEventKind::Down(MouseButton::Left), col, row);
        }
        assert!(
            !app.config.panes.visible(Pane::Playerbar),
            "the pane did not collapse"
        );
        assert!(
            app.state.pane_drag.is_none(),
            "a double click must not start a drag"
        );

        // Collapsed, the pane has no edge to click, so it is brought back by the command — the
        // same call the command line makes.
        app.config.panes.restore(Pane::Playerbar);
        assert_eq!(app.config.panes.playerbar, size);
    }

    /// `Ctrl` + an arrow moves the same edge the mouse does, and the size is clamped like a drag.
    #[tokio::test]
    async fn control_arrows_resize_the_same_edges() {
        let mut app = app_with(PanesConfig::default());
        let nav = app.config.panes.navigation;

        let ctrl = |code| {
            KeyPress::new(
                code,
                Modifiers {
                    ctrl: true,
                    ..Modifiers::NONE
                },
            )
        };
        assert!(handle_key(&mut app, ctrl(KeyCode::Right)));
        assert_eq!(app.config.panes.navigation, nav + KEY_STEP as u16);

        assert!(handle_key(&mut app, ctrl(KeyCode::Left)));
        assert_eq!(app.config.panes.navigation, nav);

        // Without `Ctrl` the arrow keys belong to whatever page is up.
        assert!(!handle_key(
            &mut app,
            KeyPress::new(KeyCode::Left, Modifiers::NONE)
        ));
    }
}
