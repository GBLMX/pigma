//! ratatui widgets and rendering helpers used by the views (tables, player bar,
//! navigation, lyrics, toasts, spinners, breadcrumbs, ...).

mod artist;
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

use ratatui::{Frame, layout::Rect, style::Style, widgets::Fill};

use crate::{
    app::App,
    config::{BorderConfig, Config, NavPosition, ThemeRegistry},
    layout,
    state::PageRender,
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

    // Mouse hit areas belong to the frame that drew them: the views that own one rebuild it,
    // so anything that does not draw is cleared here first. The navigation is the one that
    // needs this said out loud — a terminal too narrow for the sidebar hides it, and the
    // lyrics/queue/artist pages never draw it at all, yet `handle_click` asks the navigation
    // before anything else. The previous frame's areas therefore used to survive and take
    // clicks meant for whatever is drawn there now: invisible nav items hijacking the
    // content under them (measured: with the sidebar hidden, a click inside the content area
    // still selected a navigation item).
    app.state.navigation.nav.nav_hits.clear();
    app.state.nav_area = Rect::default();

    let bs = style(
        &app.config,
        &app.theme_registry,
        &app.state.border,
        app.state.tick,
    );

    // Paint the theme's background over the whole frame before anything else.
    //
    // Without this, every cell the theme does not explicitly paint keeps the terminal's own
    // colours — so a light theme in a dark terminal renders as thin light-grey text on a dark
    // screen: half a theme, which reads as no theme at all (and is what the loading list after
    // a song switch looked like). Themes are the app's visual identity, background included.
    f.render_widget(
        Fill::new(" ").style(Style::default().bg(bs.colors.bg)),
        f.area(),
    );

    match app.state.navigation.page.spec().render {
        PageRender::Standalone(draw_page) => draw_page(f, app, area),
        PageRender::Shell {
            layout: page_layout,
            content: page_content,
        } => {
            let lay = page_layout(area, app.config.navigation_position);

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

            page_content(f, app, &lay);
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

    toast::draw_toast(f, app, app.current_theme());
}

/// The styling a view draws with: the resolved theme, the border mode and the tick.
///
/// Taken field by field instead of as `&App`: a view that draws into the app borrows it
/// mutably, and one borrow of the whole app would rule that out.
fn style<'a>(
    config: &Config,
    themes: &'a ThemeRegistry,
    border: &'a BorderConfig,
    tick: u64,
) -> BlockStyle<'a> {
    BlockStyle {
        colors: App::resolve_theme(config, themes),
        border,
        tick,
    }
}

/// The splash page: it owns the whole frame.
pub(crate) fn draw_splash(f: &mut Frame, app: &mut App, area: Rect) {
    let bs = style(
        &app.config,
        &app.theme_registry,
        &app.state.border,
        app.state.tick,
    );
    let lay = layout::splash(area, splash::LOGO.len() as u16);
    splash::draw(f, &app.state.splash, &bs, &lay);
}

/// The login page: it owns the whole frame.
pub(crate) fn draw_login(f: &mut Frame, app: &mut App, area: Rect) {
    let bs = style(
        &app.config,
        &app.theme_registry,
        &app.state.border,
        app.state.tick,
    );
    let lay = layout::login(area);
    login::draw(f, &mut app.state.login, &bs, &lay);
}

/// The main page's own area: the navigation column (or row), the breadcrumb and the
/// content table.
pub(crate) fn draw_main(f: &mut Frame, app: &mut App, areas: &layout::LayoutAreas) {
    let bs = style(
        &app.config,
        &app.theme_registry,
        &app.state.border,
        app.state.tick,
    );
    match app.config.navigation_position {
        NavPosition::Left | NavPosition::Right => {
            if areas.sidebar.width > 0 {
                app.state.nav_area = areas.sidebar;
                navigation::draw(
                    f,
                    &mut app.state.navigation.nav,
                    &bs,
                    &app.config.titles.sidebar,
                    areas.sidebar,
                );
            }

            breadcrumb::render_breadcrumb(f, &app.state.navigation.nav, &bs, areas.breadcrumb);
        }
        NavPosition::Top | NavPosition::Bottom => {
            app.state.nav_area = areas.nav;
            navigation::draw_top(f, &mut app.state.navigation.nav, &bs, areas.nav);
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
    let inner = block.inner(areas.content);
    f.render_widget(block, areas.content);

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

/// The lyrics page: the scrolling lyrics, in the content area.
pub(crate) fn draw_lyrics(f: &mut Frame, app: &mut App, areas: &layout::LayoutAreas) {
    let bs = style(
        &app.config,
        &app.theme_registry,
        &app.state.border,
        app.state.tick,
    );
    lyrics::draw(
        f,
        &app.playback.state,
        &bs,
        app.config.lyric_gradient,
        app.config.lyric_style,
        &app.config.titles.lyrics,
        areas.content,
    );
}

/// The queue page: the queue tabs and their table, in the content area.
pub(crate) fn draw_queue(f: &mut Frame, app: &mut App, areas: &layout::LayoutAreas) {
    let bs = style(
        &app.config,
        &app.theme_registry,
        &app.state.border,
        app.state.tick,
    );
    queue::draw_queue_table(
        f,
        &app.playback,
        app.state.navigation.playlist_selected,
        &bs,
        &app.config.titles.playlist,
        &mut app.state.navigation.queue_tab_scroll_x,
        &mut app.state.queue_hits,
        areas.content,
    );
}

/// The artist page: one singer's profile, hot songs and albums, in the content area.
pub(crate) fn draw_artist(f: &mut Frame, app: &mut App, areas: &layout::LayoutAreas) {
    // The page's loader reports through its own channel, and this is what reads it: results
    // are taken in before the frame that shows them is drawn.
    app.state.navigation.artist.poll();

    let bs = style(
        &app.config,
        &app.theme_registry,
        &app.state.border,
        app.state.tick,
    );
    let lay = layout::artist(areas.content);
    artist::draw(f, &mut app.state.navigation.artist, &bs, &lay);
}

/// Throwaway audit: render every user-facing view for every built-in theme and report
/// glyph cells whose foreground is too close to the background under them — either
/// because both come from the theme, or because the app painted a background and left
/// the text to the terminal. A light theme in a dark terminal is where this shows up.
#[cfg(test)]
mod contrast_audit {
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
}

/// The theme has to own the background, not just the text colours.
#[cfg(test)]
mod theme_background {
    use ratatui::{Terminal, backend::TestBackend, style::Color};

    use crate::{app::App, config::Config};

    /// Every cell that draws something must sit on the theme's background.
    ///
    /// Spreading foreground colours over the terminal's own background is half a theme, and it
    /// reads as none: a light theme in a dark terminal came out as thin light-grey text on a
    /// dark screen, which is what the list looked like right after a song switch.
    #[tokio::test]
    async fn drawn_cells_never_borrow_the_terminals_background() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        for name in ["default", "github-light", "gruvbox-light", "tokyo-night"] {
            let config = Config {
                default_theme: name.to_string(),
                ..Config::default()
            };
            let mut app = match App::new(config, false) {
                Ok(app) => app,
                Err(e) => panic!("{name}: {e}"),
            };
            // The main page with its content still loading: what a song switch shows.
            app.state.navigation.page = crate::state::Page::Main;
            app.state.navigation.content = crate::state::ContentState::Loading.into();

            let mut terminal = Terminal::new(TestBackend::new(80, 20)).expect("backend");
            terminal.draw(|f| super::draw(f, &mut app)).expect("draw");
            let buffer = terminal.backend().buffer().clone();

            let mut borrowed = Vec::new();
            for y in 0..buffer.area.height {
                for x in 0..buffer.area.width {
                    let cell = &buffer[(x, y)];
                    // Blank cells are skipped: the transparent borders are deliberate, and they
                    // draw nothing.
                    if cell.symbol().trim().is_empty() {
                        continue;
                    }
                    if cell.bg == Color::Reset {
                        borrowed.push((x, y, cell.symbol().to_string()));
                    }
                }
            }
            assert!(
                borrowed.is_empty(),
                "{name}: {} cells draw on the terminal's background, e.g. {:?}",
                borrowed.len(),
                &borrowed[..borrowed.len().min(6)]
            );
        }
    }
}
/// `cargo test --release --lib -- --ignored --nocapture frame_bench`
///
/// What an **idle** frame costs. The other benches answer "does the work that runs *while music
/// plays* stay far below its frame budget"; this one answers the question the main loop raises:
/// it draws one whole frame per iteration and `handle_events` returns at least every 32 ms, so
/// this number is paid around 31 times a second **whether or not anything on screen changed**.
#[cfg(test)]
mod frame_bench {
    use ratatui::{Terminal, backend::TestBackend};

    use crate::{app::App, config::Config, state::Page};

    #[tokio::test]
    #[ignore]
    async fn one_idle_frame_costs() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut app = App::new(Config::default(), false).expect("app");
        // The main page with no song playing: the state a user leaves the app in.
        app.state.navigation.page = Page::Main;

        let mut terminal = Terminal::new(TestBackend::new(200, 50)).expect("backend");
        let per_frame =
            crate::bench_util::time("空闲整帧（主页面，无播放）", 300, || {
                terminal.draw(|f| super::draw(f, &mut app)).expect("draw");
            });
        // The loop is event-driven (`handle_events` blocks on the event stream unless the user is
        // dragging the seek bar), so this is NOT paid continuously while idle: it is what one
        // frame costs, and the analysis stream spends it about 30 times a second while playing.
        println!(
            "  → 单帧开销；播放时分析流约 30 次/秒 → 约 {:.2}% 单核",
            crate::bench_util::core_share(per_frame, 30.0)
        );
    }
}
