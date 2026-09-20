//! ratatui widgets and rendering helpers used by the views (tables, player bar,
//! navigation, lyrics, toasts, spinners, breadcrumbs, ...).

mod block;
mod breadcrumb;
mod command_panel;
mod content;
mod gradient_line_gauge;
mod help;
mod login;
mod lyrics;
mod navigation;
pub(crate) mod playerbar;
mod queue;
mod scrollbar;
mod skeleton;
mod spinner;
mod splash;
mod styled_text;
mod table;
mod title;
mod toast;
mod topbar;

use std::{sync::Arc, time::Duration};

use ratatui::Frame;

use crate::{
    app::App,
    config::NavPosition,
    layout,
    state::Page,
    ui::{
        block::{BlockStyle, CornerBlock},
        title::render_title,
    },
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let now = std::time::Instant::now();
    let steps = (now.duration_since(app.state.last_tick).as_millis() / 80).max(1) as u64;
    app.state.last_tick = now;
    app.state.tick = app.state.tick.wrapping_add(steps);

    if let Some(t) = app.state.toast_time
        && t.elapsed() > Duration::from_secs(2)
    {
        app.state.toast_time = None;
    }

    let area = f.area();

    let colors = App::resolve_theme(&app.config, &app.theme_registry);

    let bs = BlockStyle {
        colors,
        border: &app.state.border,
        tick: app.state.tick,
    };

    match app.state.navigation.page {
        Page::Splash => {
            let lay = layout::splash(area);
            splash::draw(f, &app.state.splash, &bs, &lay);
        }
        Page::Login => {
            let lay = layout::login(area);
            login::draw(f, &mut app.state.login, &bs, &lay);
        }
        page => {
            let lay = layout::build_layout(area, page, app.config.navigation_position);

            topbar::draw(
                f,
                app.state.navigation.user.as_ref(),
                &app.state.navigation.search,
                &app.state.prompt,
                &bs,
                lay.topbar,
            );
            app.state.playerbar_area = lay.playerbar;
            let is_sixel = app.picker.protocol_type() == ratatui_image::picker::ProtocolType::Sixel;
            let playerbar_areas = playerbar::draw(
                f,
                &app.playback.state,
                app.state.tick,
                &bs,
                &app.config.playerbar,
                lay.playerbar,
                is_sixel,
            );
            app.state.gauge_area = playerbar_areas.gauge;
            app.state.volume_area = playerbar_areas.volume;
            app.state.cover_area = playerbar_areas.cover;
            app.state.spectrum_row_area = playerbar_areas.visualizer;
            app.state.pitch_area = playerbar_areas.pitch;
            app.state.transport = playerbar::control_rects(
                playerbar_areas.controls,
                playerbar_areas.controls_centered,
            );
            app.state.mode_area =
                playerbar::mode_icon_rect(&app.playback.state, playerbar_areas.mode_icon);
            app.state.like_areas = [
                playerbar::like_rect(&app.playback.state, playerbar_areas.song_detail),
                playerbar::song_info_like_rect(&app.playback.state, playerbar_areas.song_info),
            ];

            match page {
                Page::Main => {
                    match app.config.navigation_position {
                        NavPosition::Left | NavPosition::Right => {
                            if lay.sidebar.width > 0 {
                                navigation::draw(
                                    f,
                                    &mut app.state.navigation.nav,
                                    &bs,
                                    &app.config.titles.sidebar,
                                    lay.sidebar,
                                );
                            }

                            breadcrumb::render_breadcrumb(
                                f,
                                &app.state.navigation.nav,
                                &bs,
                                lay.breadcrumb,
                            );
                        }
                        NavPosition::Top | NavPosition::Bottom => {
                            navigation::draw_top(f, &mut app.state.navigation.nav, &bs, lay.nav);
                        }
                    }

                    let nav = &app.state.navigation.nav;
                    let current_item = nav.selected_item();

                    let title = {
                        let nst = &app.state.navigation;
                        let focus = nst.nav.focus_section;
                        let selected = nst.nav.selected_index();
                        let generation = nst.generation;
                        let count = nst.content.len();
                        let cached = nst.title_cache.borrow();
                        if let Some((ref title, f, s, g, c)) = *cached
                            && f == focus
                            && s == selected
                            && g == generation
                            && c == count
                        {
                            Arc::clone(title)
                        } else {
                            drop(cached);
                            let name = current_item
                                .map(|item| item.name.as_str())
                                .unwrap_or("SONGS");
                            let total = nst
                                .pagination
                                .as_ref()
                                .map(|p| p.total as usize)
                                .unwrap_or(count);
                            // When content is paged (total known), show `count/total` like the
                            // music cloud drive; non-paged content shows only the loaded count.
                            // Each nav item can also override this via `title_template`.
                            let show_total = nst.pagination.as_ref().is_some_and(|p| p.total > 0);
                            let template = current_item
                                .and_then(|item| item.title_template.as_deref())
                                .unwrap_or(if show_total {
                                    "\u{25BA} {name} ({count}/{total}) \u{25C4}"
                                } else {
                                    "\u{25BA} {name} ({count}) \u{25C4}"
                                });
                            let title = Arc::new(render_title(template, name, count, total));
                            *nst.title_cache.borrow_mut() =
                                Some((Arc::clone(&title), focus, selected, generation, count));
                            title
                        }
                    };
                    let block = CornerBlock::from_color(&bs, bs.colors.bg).title(&title, bs.colors);
                    let inner = block.inner(lay.content);
                    f.render_widget(block, lay.content);

                    let api = nav.selected_api();

                    let content_offset = content::render_content(
                        f,
                        &app.state.navigation.content,
                        &app.config.columns,
                        api,
                        &bs,
                        &mut app.state.navigation.table_state,
                        app.state.navigation.content_selected,
                        app.state.navigation.table_mode,
                        inner,
                    );
                    // Remembered for mouse input: which rows are on screen, and where.
                    app.state.content_inner = inner;
                    app.state.content_offset = content_offset;
                }
                Page::Lyrics => {
                    lyrics::draw(
                        f,
                        &app.playback.state,
                        &bs,
                        app.config.lyric_gradient,
                        &app.config.titles.lyrics,
                        lay.content,
                    );
                }
                Page::Playlist => {
                    queue::draw_queue_table(
                        f,
                        &app.playback,
                        app.state.navigation.playlist_selected,
                        &bs,
                        &app.config.titles.playlist,
                        &mut app.state.navigation.queue_tab_scroll_x,
                        &mut app.state.queue_hits,
                        lay.content,
                    );
                }
                _ => {}
            }
        }
    }

    if app.state.command_panel.open {
        command_panel::draw(f, app, area);
    }

    if app.state.help.open {
        // Persist the rendered scroll limit so scroll_down clamps at the real
        // bottom; clamping scroll here also heals drift after a resize.
        let max_scroll = help::draw(f, app, area);
        app.state.help.max_scroll = max_scroll;
        app.state.help.scroll = app.state.help.scroll.min(max_scroll);
    }

    toast::draw_toast(f, app, colors);
}

/// Throwaway audit: render every user-facing view for every built-in theme and report
/// glyph cells whose foreground is too close to the background under them — either
/// because both come from the theme, or because the app painted a background and left
/// the text to the terminal. A light theme in a dark terminal is where this shows up.
#[cfg(test)]
mod contrast_audit {
    use ratatui::{Terminal, backend::TestBackend, style::Color};

    use crate::{app::App, config::Config};

    fn to_rgb(color: Color) -> Option<(f64, f64, f64)> {
        match color {
            Color::Rgb(r, g, b) => Some((r as f64, g as f64, b as f64)),
            _ => None,
        }
    }

    fn luminance((r, g, b): (f64, f64, f64)) -> f64 {
        let f = |c: f64| {
            let c = c / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)
    }

    fn contrast(fg: (f64, f64, f64), bg: (f64, f64, f64)) -> f64 {
        let (la, lb) = (luminance(fg), luminance(bg));
        let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    fn audit(label: &str, app: &mut App, theme: &crate::config::Theme) {
        let mut terminal = Terminal::new(TestBackend::new(120, 32)).expect("backend");
        terminal.draw(|f| super::draw(f, app)).expect("draw");
        let buffer = terminal.backend().buffer().clone();
        let theme_bg = to_rgb(theme.bg);
        let theme_fg = to_rgb(theme.text);

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
                let fg = to_rgb(cell.fg).or(theme_fg);
                let bg = to_rgb(cell.bg).or(theme_bg);
                let (Some(fg), Some(bg)) = (fg, bg) else {
                    continue;
                };
                let r = contrast(fg, bg);
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
            if !name.contains("light") && !name.contains("latte") && name != "solarized" {
                continue;
            }
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
}
