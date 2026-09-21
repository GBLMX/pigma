//! ratatui widgets and rendering helpers used by the views (tables, player bar,
//! navigation, lyrics, toasts, spinners, breadcrumbs, ...).

mod artist;
pub(crate) mod block;
mod breadcrumb;
mod command_panel;
mod content;
mod gradient_line_gauge;
mod help;
mod login;
mod lyrics;
mod messages;
mod navigation;
pub(crate) mod playerbar;
mod queue;
mod scrollbar;
pub(crate) mod settings;
mod skeleton;
mod spinner;
mod splash;
mod styled_text;
mod table;
mod tasks;
mod title;
mod toast;
mod topbar;

use std::{sync::Arc, time::Instant};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::Fill,
};

use crate::{
    app::App,
    config::{BorderConfig, Config, NavPosition, ThemeRegistry},
    layout,
    state::{PageRender, lyrics as lyrics_state},
    ui::{
        block::{BlockStyle, CornerBlock},
        title::render_title,
    },
    utils::terminal::Background,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let now = std::time::Instant::now();
    let steps = (now.duration_since(app.state.last_tick).as_millis() / 80).max(1) as u64;
    app.state.last_tick = now;
    app.state.tick = app.state.tick.wrapping_add(steps);

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
    // The frame's draggable pane edges are rebuilt with the frame, like every other hit area.
    app.state.pane_dividers.clear();
    app.state.shell_area = area;

    let bs = style(
        &app.config,
        &app.theme_registry,
        &app.state.border,
        app.terminal_background,
        app.state.tick,
    );

    // Paint the background behind everything, before anything else, and let `base` decide which
    // background that is.
    //
    // Without a fill at all, every cell the theme does not explicitly paint keeps the terminal's
    // own colours — so a light theme in a dark terminal renders as thin light-grey text on a dark
    // screen: half a theme, which reads as no theme at all (and is what the loading list after a
    // song switch looked like). A background the terminal already has, though, is a fill nobody
    // can see doing something everybody can: it covers the terminal's own background, and with it
    // whatever the user put there — a translucent background, Windows Terminal's acrylic. So
    // `base` is the theme's background only when the two disagree, and `Reset` (the terminal's
    // own, and a no-op write into a fresh buffer) otherwise: see `BackgroundFill`.
    f.render_widget(Fill::new(" ").style(Style::default().bg(bs.base)), f.area());

    match app.state.navigation.page.spec().render {
        PageRender::Standalone(draw_page) => draw_page(f, app, area),
        PageRender::Shell {
            layout: page_layout,
            content: page_content,
        } => {
            let lay = page_layout(area, &app.config.panes, app.config.navigation_position);
            app.state.pane_dividers = lay.dividers;

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

    if app.state.messages.open {
        let max_scroll = messages::draw(f, app, area);
        app.state.messages.max_scroll = max_scroll;
        app.state.messages.scroll = app.state.messages.scroll.min(max_scroll);
    }

    if app.state.tasks_popup.open {
        let max_scroll = tasks::draw(f, app, area);
        app.state.tasks_popup.max_scroll = max_scroll;
        app.state.tasks_popup.scroll = app.state.tasks_popup.scroll.min(max_scroll);
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

/// The styling a view draws with: the resolved theme, the background to paint behind content,
/// the border mode and the tick.
///
/// Taken field by field instead of as `&App`: a view that draws into the app borrows it
/// mutably, and one borrow of the whole app would rule that out.
fn style<'a>(
    config: &Config,
    themes: &'a ThemeRegistry,
    border: &'a BorderConfig,
    terminal_background: Background,
    tick: u64,
) -> BlockStyle<'a> {
    let colors = App::resolve_theme(config, themes);

    // Whether the theme's background goes on top of the terminal's is decided once, here: the
    // answer is what every widget paints behind its content (`BlockStyle::base`), so a
    // translucent terminal stays translucent from the frame to the last row.
    let base = if config
        .paint_background
        .paints(colors.background(), terminal_background)
    {
        colors.bg
    } else {
        Color::Reset
    };

    BlockStyle {
        colors,
        base,
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
        app.terminal_background,
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
        app.terminal_background,
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
        app.terminal_background,
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
    let block = CornerBlock::from_color(&bs, bs.base).title(&title, bs.colors);
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
    // The flow's colour advances here, by the clock, before the page reads it: one pass takes one
    // line, so its speed comes from the line being sung, and the phase is carried across line
    // changes rather than derived from the frame count.
    advance_flow(app);

    let bs = style(
        &app.config,
        &app.theme_registry,
        &app.state.border,
        app.terminal_background,
        app.state.tick,
    );
    // What the page is asked to draw, resolved against the theme in force right now — the page
    // cannot know which theme is active, and the colour is a theme field name or a colour of its
    // own.
    let lyrics_config = app.config.lyrics_config(bs.colors);
    lyrics::draw(
        f,
        &app.playback.state,
        &bs,
        &lyrics_config,
        &mut app.state.lyrics,
        &app.config.panes,
        &mut app.state.pane_dividers,
        areas.content,
    );
}

/// Move the lyrics page's flow on to now: one pass of the palette per line of the song.
///
/// The phase belongs to the `App` rather than to the page, so a line change does not restart it,
/// and it is advanced with the wall clock rather than with the frame counter: the flow's speed
/// follows the line being sung, which a frame count cannot know.
fn advance_flow(app: &mut App) {
    let player = &app.playback.state;
    let Some(lyrics) = player.lyrics.as_deref().filter(|lines| !lines.is_empty()) else {
        return;
    };

    let cur_ms = player.position_secs * 1000.0;
    let total_ms = player
        .current_song
        .as_ref()
        .and_then(|song| lyrics_state::song_duration_ms(song.duration));
    let line = app.state.lyrics.current_line(lyrics, cur_ms);
    let line_ms = lyrics_state::line_duration(lyrics, line, total_ms);
    let playing = player.playing && !player.paused;

    app.state.lyrics.flow(playing, line_ms, Instant::now());
}

/// The queue page: the queue tabs and their table, in the content area.
pub(crate) fn draw_queue(f: &mut Frame, app: &mut App, areas: &layout::LayoutAreas) {
    let bs = style(
        &app.config,
        &app.theme_registry,
        &app.state.border,
        app.terminal_background,
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

/// The settings page: every switch the app has, in the content area.
pub(crate) fn draw_settings(f: &mut Frame, app: &mut App, areas: &layout::LayoutAreas) {
    let bs = style(
        &app.config,
        &app.theme_registry,
        &app.state.border,
        app.terminal_background,
        app.state.tick,
    );
    settings::draw(f, &app.config, &mut app.state.settings, &bs, areas.content);
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
        app.terminal_background,
        app.state.tick,
    );
    let lay = layout::artist(areas.content);
    artist::draw(
        f,
        &mut app.state.navigation.artist,
        &bs,
        &lay,
        &mut app.state.artist_hits,
    );
}


/// Regenerate the README's screenshots.
///
/// The pages are drawn by the app itself, offscreen, into the same `TestBackend` the tests use —
/// so a screenshot is what the app really renders, with a real theme, and can be regenerated
/// when the UI changes. Each page is written as HTML (one `<span>` per run of cells that share a
/// colour) next to a plain-text dump, and the HTML is what gets rasterised into `imgs/`.
///
/// `#[ignore]`d because it writes files: run it deliberately.
#[cfg(test)]
mod shots {
    use std::{fmt::Write as _, fs, path::PathBuf};

    use ratatui::{
        Terminal,
        backend::TestBackend,
        style::{Color, Modifier},
    };
    use unicode_width::UnicodeWidthStr;

    use crate::{
        app::App,
        config::Config,
        state::{ArtistData, ContentState, Page},
        ui,
        utils::terminal::{ANSI_16, palette_rgb},
    };

    /// Cells per side: with the previews' 8×16 px cell that is 1280×768 — a terminal someone
    /// might actually run, and wide enough for the panes to lay out the way the preview shows.
    const COLS: u16 = 160;
    const ROWS: u16 = 48;

    /// The colour a ratatui `Color` stands for in CSS, or `None` for "whatever is behind it"
    /// (`Color::Reset`), which the page's own background answers.
    fn css(color: Color) -> Option<String> {
        let (r, g, b) = match color {
            Color::Reset => return None,
            Color::Rgb(r, g, b) => (r, g, b),
            Color::Indexed(index) => palette_rgb(index),
            Color::Black => ANSI_16[0],
            Color::Red => ANSI_16[1],
            Color::Green => ANSI_16[2],
            Color::Yellow => ANSI_16[3],
            Color::Blue => ANSI_16[4],
            Color::Magenta => ANSI_16[5],
            Color::Cyan => ANSI_16[6],
            Color::Gray => ANSI_16[7],
            Color::DarkGray => ANSI_16[8],
            Color::LightRed => ANSI_16[9],
            Color::LightGreen => ANSI_16[10],
            Color::LightYellow => ANSI_16[11],
            Color::LightBlue => ANSI_16[12],
            Color::LightMagenta => ANSI_16[13],
            Color::LightCyan => ANSI_16[14],
            Color::White => ANSI_16[15],
        };
        Some(format!("#{r:02x}{g:02x}{b:02x}"))
    }

    fn escape(symbol: &str, out: &mut String) {
        for ch in symbol.chars() {
            match ch {
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '&' => out.push_str("&amp;"),
                ' ' => out.push_str("&nbsp;"),
                _ => out.push(ch),
            }
        }
    }

    /// The whole frame as one `<pre>`, one `<span>` per run of cells that share their look.
    fn html(buffer: &ratatui::buffer::Buffer, background: Color, foreground: Color) -> String {
        let bg = css(background).unwrap_or_else(|| "#000000".to_string());
        let fg = css(foreground).unwrap_or_else(|| "#ffffff".to_string());
        let mut out = String::new();
        let _ = write!(
            out,
            "<!doctype html><meta charset=\"utf-8\">\
             <style>html,body{{margin:0;padding:0;background:{bg}}}\
             pre{{margin:0;padding:0;font:16px/16px 'MS Gothic',monospace;\
             white-space:pre;background:{bg};color:{fg}}}</style><pre>"
        );

        for y in 0..buffer.area.height {
            let mut run: Option<(Option<String>, Option<String>, Modifier)> = None;
            let mut text = String::new();
            let flush = |run: &mut Option<(Option<String>, Option<String>, Modifier)>,
                             text: &mut String,
                             out: &mut String| {
                if let Some((fg, bg, modifier)) = run.take() {
                    let mut style = String::new();
                    if let Some(fg) = fg {
                        let _ = write!(style, "color:{fg};");
                    }
                    if let Some(bg) = bg {
                        let _ = write!(style, "background:{bg};");
                    }
                    if modifier.contains(Modifier::BOLD) {
                        style.push_str("font-weight:bold;");
                    }
                    if modifier.contains(Modifier::ITALIC) {
                        style.push_str("font-style:italic;");
                    }
                    if modifier.contains(Modifier::UNDERLINED) {
                        style.push_str("text-decoration:underline;");
                    }
                    if modifier.contains(Modifier::DIM) {
                        style.push_str("opacity:0.6;");
                    }
                    if modifier.contains(Modifier::REVERSED) {
                        style.push_str("filter:invert(1);");
                    }
                    let _ = write!(out, "<span style=\"{style}\">");
                    escape(text, out);
                    out.push_str("</span>");
                }
                text.clear();
            };

            // A wide glyph occupies two cells, and ratatui fills the second one with a space.
            // The font draws the glyph two cells wide on its own, so that space would be a
            // third cell and every column after it would drift.
            let mut continuation = false;
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                let symbol = cell.symbol();
                if continuation {
                    continuation = false;
                    continue;
                }
                let look = (css(cell.fg), css(cell.bg), cell.modifier);
                if run.as_ref() != Some(&look) {
                    flush(&mut run, &mut text, &mut out);
                    run = Some(look);
                }
                text.push_str(symbol);
                continuation = UnicodeWidthStr::width(symbol) >= 2;
            }
            flush(&mut run, &mut text, &mut out);
            out.push('\n');
        }
        out.push_str("</pre>\n");
        out
    }

    fn out_dir() -> PathBuf {
        let dir = PathBuf::from("target/shots");
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn shoot(name: &str, setup: fn(&mut App)) {
        let mut app = App::new(Config::default(), false).expect("app");
        // A screenshot is published: it must not carry anyone's account. The app reads the
        // real config directory, so the logged-in user has to be dropped before anything is
        // drawn — the topbar shows their name otherwise.
        app.state.navigation.user = None;
        setup(&mut app);
        app.state.navigation.user = None;
        // The app starts on the splash; a preview is of the page behind it unless the setup
        // asked for another one.
        if app.state.navigation.page == Page::Splash {
            app.state.navigation.page = Page::Main;
        }
        let theme = app.current_theme().clone();
        let mut terminal = Terminal::new(TestBackend::new(COLS, ROWS)).expect("backend");
        terminal.draw(|f| ui::draw(f, &mut app)).expect("draw");
        let buffer = terminal.backend().buffer().clone();

        let mut text = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        let dir = out_dir();
        fs::write(dir.join(format!("{name}.html")), html(&buffer, theme.bg, theme.text))
            .expect("html");
        fs::write(dir.join(format!("{name}.txt")), text).expect("text");
        println!("  {} -> {}", name, dir.join(format!("{name}.html")).display());
    }

    /// A handful of songs, so the tables are not empty.
    fn songs(count: u64) -> Vec<std::sync::Arc<ncm_api::SongInfo>> {
        (1..=count)
            .map(|id| {
                std::sync::Arc::new(ncm_api::SongInfo {
                    id,
                    name: format!("夜航星 {id}"),
                    singer: "不才".into(),
                    artist_id: 0,
                    album: "三体 电视剧原声带".into(),
                    album_id: 0,
                    pic_url: String::new(),
                    duration: 240_000,
                    mv: 0,
                    copyright: ncm_api::SongCopyright::Free,
                    local_path: None,
                })
            })
            .collect()
    }

    #[tokio::test]
    #[ignore = "writes target/shots/*.html for the README's previews"]
    async fn render_the_readme_previews() {
        let _ = rustls::crypto::ring::default_provider().install_default();

        shoot("main", |app| {
            app.state.navigation.set_content(ContentState::Songs(songs(18)));
        });
        shoot("artist", |app| {
            app.state.navigation.page = Page::Artist;
            let detail = ncm_api::ArtistDetail {
                id: 6452,
                name: "周杰伦".into(),
                alias: vec!["Jay Chou".into()],
                brief_desc: "华语流行歌手、音乐人。".into(),
                pic_url: String::new(),
                album_size: 42,
                music_size: 511,
                hot_songs: songs(12)
                    .into_iter()
                    .map(|song| (*song).clone())
                    .collect(),
            };
            app.state.navigation.artist.id = 6452;
            app.state.navigation.artist.name = "周杰伦".into();
            app.state.navigation.artist.data = ArtistData::Ready {
                detail,
                albums: Ok((1..=8)
                    .map(|n| ncm_api::ArtistAlbum {
                        id: n,
                        name: format!("专辑 {n}"),
                        pic_url: String::new(),
                        size: 10 + n,
                        publish_time: 1_500_000_000_000 + n * 86_400_000,
                    })
                    .collect()),
                similar: Ok(vec![
                    ncm_api::SingerInfo {
                        id: 6452,
                        name: "林俊杰".into(),
                        pic_url: String::new(),
                    },
                    ncm_api::SingerInfo {
                        id: 2,
                        name: "王力宏".into(),
                        pic_url: String::new(),
                    },
                    ncm_api::SingerInfo {
                        id: 3,
                        name: "陈奕迅".into(),
                        pic_url: String::new(),
                    },
                ]),
            };
        });
        shoot("settings", |app| {
            app.state.navigation.page = Page::Settings;
        });
        shoot("queue", |app| {
            app.state.navigation.page = Page::Playlist;
            app.playback
                .set_queue_songs(songs(24).into_iter().collect());
        });
        shoot("panel", |app| {
            app.state.navigation.set_content(ContentState::Songs(songs(18)));
            app.state.command_panel.open = true;
        });
    }
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
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};

    use crate::{
        app::App,
        config::Config,
        utils::terminal::{Background, BackgroundFill},
    };

    /// A frame drawn on a terminal whose own background is `terminal`, with `fill` deciding
    /// whether the theme's background is painted over it — and the theme's own background colour.
    ///
    /// The theme is a *light* one (`github-light`, whichever slot the config resolves to), so
    /// "the theme and the terminal agree" is a state a test can set on any machine.
    fn frame(fill: BackgroundFill, terminal: Background) -> (Buffer, Color) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let config = Config {
            default_theme: "github-light".to_string(),
            paint_background: fill,
            ..Config::default()
        };
        let mut app = App::new(config, false).expect("app");
        app.terminal_background = terminal;
        // The main page with its content still loading: what a song switch shows.
        app.state.navigation.page = crate::state::Page::Main;
        app.state.navigation.content = crate::state::ContentState::Loading.into();

        let theme = app
            .theme_registry
            .get("github-light")
            .expect("a built-in theme")
            .clone();
        assert_eq!(
            theme.background(),
            Background::Light,
            "the fixture's theme must be the light one"
        );

        let mut terminal = Terminal::new(TestBackend::new(80, 20)).expect("backend");
        terminal.draw(|f| super::draw(f, &mut app)).expect("draw");

        (terminal.backend().buffer().clone(), theme.bg)
    }

    /// Cells that leave the background to the terminal: what a translucent terminal shows
    /// through, and what acrylic blurs.
    ///
    /// The blank cell a wide glyph leaves behind it (`未` and the half-cell ratatui writes after
    /// it) is deliberately not counted: it carries no style of its own because the terminal
    /// paints it as part of the glyph, so nobody sees the terminal's background through it.
    fn transparent(buffer: &Buffer) -> Vec<(u16, u16, String)> {
        use unicode_width::UnicodeWidthStr;

        let mut cells = Vec::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                if buffer[(x, y)].bg != Color::Reset {
                    continue;
                }
                let after_wide = x > 0 && UnicodeWidthStr::width(buffer[(x - 1, y)].symbol()) > 1;
                if !after_wide {
                    cells.push((x, y, buffer[(x, y)].symbol().to_string()));
                }
            }
        }
        cells
    }

    /// `always`: the theme's background over every cell, which is the fix for a light theme in a
    /// dark terminal — thin light-grey text on a dark screen without it.
    #[tokio::test]
    async fn always_paints_the_theme_over_every_cell() {
        let (buffer, _) = frame(BackgroundFill::Always, Background::Dark);

        let showing = transparent(&buffer);
        assert!(
            showing.is_empty(),
            "{} cells still show the terminal's background, e.g. {:?}",
            showing.len(),
            &showing[..showing.len().min(6)]
        );
    }

    /// The default: with the theme's own background already the terminal's, the fill is
    /// invisible while what it hides — the terminal's transparency — is not. So the base stays
    /// the terminal's, and the surfaces the theme *does* paint are still painted.
    #[tokio::test]
    async fn auto_leaves_a_matching_background_to_the_terminal() {
        let (buffer, bg) = frame(BackgroundFill::Auto, Background::Light);

        assert!(
            !transparent(&buffer).is_empty(),
            "a matching theme painted over the terminal's own background anyway"
        );
        assert!(
            buffer
                .content
                .iter()
                .any(|cell| cell.bg != Color::Reset && cell.bg != bg),
            "nothing was painted at all: transparency is not the same as no theme"
        );
    }

    /// …and when the two disagree, the fill is what keeps the page readable.
    #[tokio::test]
    async fn auto_paints_when_the_theme_and_the_terminal_disagree() {
        let (buffer, _) = frame(BackgroundFill::Auto, Background::Dark);

        let showing = transparent(&buffer);
        assert!(
            showing.is_empty(),
            "a light theme on a dark terminal has to paint its background, but {} cells \
             still show the terminal's, e.g. {:?}",
            showing.len(),
            &showing[..showing.len().min(6)]
        );
    }

    /// `never`: the terminal keeps its background whatever the theme says — the mode for someone
    /// who wants their blur more than the theme's background.
    #[tokio::test]
    async fn never_leaves_the_terminal_alone() {
        let (buffer, _) = frame(BackgroundFill::Never, Background::Dark);

        assert!(
            !transparent(&buffer).is_empty(),
            "the background was painted anyway"
        );
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
