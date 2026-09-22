//! Throwaway audit: render every user-facing view for every built-in theme and report
//! glyph cells whose foreground is too close to the background under them — either
//! because both come from the theme, or because the app painted a background and left
//! the text to the terminal. A light theme in a dark terminal is where this shows up.

use ratatui::{Terminal, backend::TestBackend};

use crate::{
    app::App,
    config::{
        Config,
        theme::{contrast_ratio, relative_luminance},
    },
};

fn audit(label: &str, app: &mut App, theme: &crate::config::Theme) {
    let mut terminal = Terminal::new(TestBackend::new(120, 32)).expect("backend");
    terminal.draw(|f| super::draw(f, app)).expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let theme_bg = relative_luminance(theme.bg);
    let theme_fg = relative_luminance(theme.text);

    let mut invisible: Vec<String> = Vec::new();
    let mut low: Vec<String> = Vec::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            let symbol = cell.symbol();
            if symbol.trim().is_empty() {
                continue;
            }
            // What the terminal would actually end up showing.
            let fg = relative_luminance(cell.fg).or(theme_fg);
            let bg = relative_luminance(cell.bg).or(theme_bg);
            let (Some(fg), Some(bg)) = (fg, bg) else {
                continue;
            };
            let r = contrast_ratio(fg, bg);
            let entry = format!(
                "({x},{y}){symbol:?} fg={:?} bg={:?} {r:.2}",
                cell.fg, cell.bg
            );
            if r < 1.6 {
                invisible.push(entry);
            } else if r < 2.5 {
                low.push(entry);
            }
        }
    }
    invisible.dedup();
    low.dedup();
    println!(
        "  {label:<26} 几乎不可见 {:>3}  偏低 {:>3}   {:?}",
        invisible.len(),
        low.len(),
        invisible
            .iter()
            .chain(low.iter())
            .take(5)
            .collect::<Vec<_>>()
    );
}

/// One user-facing view: a label and the state that makes it visible.
type View = (&'static str, fn(&mut App));

#[tokio::test]
#[ignore = "audit"]
async fn every_theme_and_view_is_readable() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let registry = crate::config::ThemeRegistry::new(Default::default());
    let names: Vec<String> = {
        let mut n: Vec<String> = registry.all_names().iter().map(|n| n.to_string()).collect();
        n.sort();
        n
    };

    for name in names {
        let config = Config {
            default_theme: name.clone(),
            ..Config::default()
        };
        let theme = registry.get(&name).cloned().unwrap_or_default();

        // Each user-facing view, audited on its own.
        let views: Vec<View> = vec![
            ("主界面", |_app| {}),
            ("帮助/操作方式", |app| {
                app.state.help.toggle();
            }),
            (": 命令行", |app| {
                app.state.prompt.active = true;
            }),
            ("命令面板", |app| {
                app.state.command_panel.open = true;
            }),
            ("登录页", |app| {
                app.state.navigation.page = crate::state::Page::Login;
            }),
            ("队列页", |app| {
                app.state.navigation.page = crate::state::Page::Playlist;
            }),
        ];

        for (view, setup) in views {
            let mut app = match App::new(config.clone(), false) {
                Ok(app) => app,
                Err(e) => {
                    println!("  {name} 无法构造: {e}");
                    break;
                }
            };
            setup(&mut app);
            audit(&format!("{name} / {view}"), &mut app, &theme);
        }
    }
}
