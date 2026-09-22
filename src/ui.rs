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

#[cfg(test)]
mod shots;

#[cfg(test)]
mod contrast_audit;

#[cfg(test)]
mod theme_background;

#[cfg(test)]
mod frame_bench;
