use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEventKind};

use super::{
    content::{
        cell_enter_action, check_load_more, content_item_count, content_select_first,
        content_select_last, content_select_next, content_select_prev, playlist_play_selected,
        playlist_select_first, playlist_select_last, playlist_select_next, playlist_select_prev,
        row_enter_action,
    },
    hit,
    navigation::{emit_nav_select, navigate_nav_down, navigate_nav_up},
    table::{cell_select_next_column, cell_select_prev_column, toggle_table_mode},
};
use crate::{
    app::App,
    config::symbols,
    event::{AppEvent, CommandEvent, NavigationEvent, PlaybackEvent},
    playback::mode_icon,
    state::{ContentState, Page, TableMode},
    text_input::TextInput,
    ui::playerbar,
};

pub(super) fn handle_main_key(app: &mut App, key_event: KeyEvent) -> color_eyre::Result<()> {
    match key_event.code {
        KeyCode::Esc => {
            app.state.events.send(NavigationEvent::ContentRestore);
        }
        KeyCode::Char('q') => app.state.events.send(AppEvent::Quit),
        KeyCode::Tab if app.state.navigation.page == Page::Playlist => {
            if let Some(key) = app.playback.switch_queue(true) {
                app.state.navigation.playlist_selected =
                    app.playback.queue_current_index().unwrap_or(0);
                app.toast(format!("▣ 队列: {key}"));
            }
        }
        KeyCode::BackTab if app.state.navigation.page == Page::Playlist => {
            if let Some(key) = app.playback.switch_queue(false) {
                app.state.navigation.playlist_selected =
                    app.playback.queue_current_index().unwrap_or(0);
                app.toast(format!("▣ 队列: {key}"));
            }
        }
        KeyCode::Tab => navigate_nav_down(app),
        KeyCode::BackTab => navigate_nav_up(app),
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            if app.state.navigation.page == Page::Playlist {
                playlist_select_prev(app);
            } else {
                content_select_prev(app);
            }
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            if app.state.navigation.page == Page::Playlist {
                playlist_select_next(app);
            } else {
                content_select_next(app);
            }
        }
        KeyCode::Char('g') => {
            if app.state.navigation.page == Page::Playlist {
                playlist_select_first(app);
            } else {
                content_select_first(app);
            }
        }
        KeyCode::Char('G') => {
            if app.state.navigation.page == Page::Playlist {
                playlist_select_last(app);
            } else {
                content_select_last(app);
            }
        }
        KeyCode::Enter => {
            if app.state.navigation.page == Page::Playlist {
                playlist_play_selected(app);
            } else if app.state.navigation.table_mode == TableMode::Cell {
                cell_enter_action(app);
            } else {
                row_enter_action(app);
            }
        }
        KeyCode::Left => {
            if app.state.navigation.table_mode == TableMode::Cell
                && app.state.navigation.page != Page::Playlist
            {
                cell_select_prev_column(app);
            } else if app.playback.current_song().is_some() {
                let interval = app.config.seek_interval_secs as f64;
                app.playback.seek_relative(-interval);
            }
        }
        KeyCode::Right => {
            if app.state.navigation.table_mode == TableMode::Cell
                && app.state.navigation.page != Page::Playlist
            {
                cell_select_next_column(app);
            } else if app.playback.current_song().is_some() {
                let interval = app.config.seek_interval_secs as f64;
                app.playback.seek_relative(interval);
            }
        }
        KeyCode::Char('l') => {
            let next = match app.state.navigation.page {
                Page::Main => Page::Lyrics,
                Page::Lyrics => Page::Main,
                Page::Playlist => Page::Main,
                Page::Login => Page::Main,
                Page::Splash => Page::Splash,
            };
            app.state.events.send(NavigationEvent::Navigate(next));
        }
        KeyCode::Char('p' | 'P') => {
            app.playback.prev();
        }
        KeyCode::Char('n' | 'N') => {
            app.playback.next();
        }
        KeyCode::Char('c' | 'C') => {
            toggle_table_mode(app);
        }
        KeyCode::Char('f' | 'F') => {
            let next = match app.state.navigation.page {
                Page::Main => {
                    app.state.navigation.playlist_selected =
                        app.playback.queue_current_index().unwrap_or(0);
                    Page::Playlist
                }
                Page::Playlist => Page::Main,
                Page::Lyrics => Page::Main,
                Page::Login => Page::Main,
                Page::Splash => Page::Splash,
            };
            app.state.events.send(NavigationEvent::Navigate(next));
        }
        KeyCode::Char('/') => {
            if app.state.navigation.page == Page::Playlist {
                app.state.navigation.search.filter_queue_only = true;
                let songs = app.playback.queue_songs();
                app.state.navigation.search.unfiltered_songs = Some(songs.to_vec());
                app.state.navigation.search.active = true;
                app.state.navigation.search.input = TextInput::new();
            } else if app.state.navigation.page == Page::Lyrics {
                return Ok(());
            } else {
                app.state.events.send(NavigationEvent::SearchActivated);
            }
        }
        KeyCode::Char('b' | 'B') => {
            app.state.events.send(CommandEvent::ToggleBordered);
        }
        KeyCode::Char(' ') => {
            let was_paused = app.playback.state.paused;
            app.playback.toggle_pause();
            if let Some(song) = app.playback.current_song() {
                if was_paused {
                    app.toast(format!("\u{f03e4}  {}", song.name));
                } else {
                    app.toast(format!("\u{f040a}  {}", song.name));
                }
            }
        }
        KeyCode::Char('m') => {
            let mode = app.playback.cycle_mode();
            let (icon, label) = mode_icon(&mode);
            app.toast(format!("{icon} 循环: {label}"));
        }
        KeyCode::Char('S') => {
            if let Some(song) = app.playback.current_song() {
                app.state
                    .events
                    .send(PlaybackEvent::LikeSong(song.id, true));
                app.toast(format!("♥  {}", song.name));
            }
        }
        KeyCode::Char('s') => {
            if let ContentState::Songs(songs) = app.state.navigation.content.as_ref() {
                let sel = app.state.navigation.content_selected;
                if let Some(song) = songs.get(sel) {
                    app.state
                        .events
                        .send(PlaybackEvent::LikeSong(song.id, true));
                    app.toast(format!("♥  {}", song.name));
                }
            }
        }
        KeyCode::Char('a' | 'A') => {
            let song =/* if app.state.navigation.page == Page::Playlist {
                app.playback
                    .song_at(app.state.navigation.playlist_selected)
                    .cloned()
            } else */ if let ContentState::Songs(songs) = app.state.navigation.content.as_ref() {
                songs
                    .get(app.state.navigation.content_selected)
                    .cloned()
            } else {
                None
            };
            if let Some(song) = song {
                app.playback.add_next(song.clone());
                app.toast(format!("⏭  下一首: {}", song.name));
            }
        }
        KeyCode::Char('d') => {
            if is_daily_recommend(app)
                && let ContentState::Songs(songs) = app.state.navigation.content.as_ref()
            {
                let sel = app.state.navigation.content_selected;
                if let Some(song) = songs.get(sel) {
                    app.state.events.send(PlaybackEvent::DislikeSong(song.id));
                    app.toast(format!("✕  {}", song.name));
                }
            } else if let ContentState::Songs(songs) = app.state.navigation.content.as_ref() {
                let sel = app.state.navigation.content_selected;
                if let Some(song) = songs.get(sel) {
                    app.state
                        .events
                        .send(PlaybackEvent::LikeSong(song.id, false));
                    app.toast(format!("♡ 已取消喜欢: {}", song.name));
                }
            }
        }
        KeyCode::Char('D') => {
            if let Some(song) = app.playback.current_song() {
                app.state
                    .events
                    .send(PlaybackEvent::LikeSong(song.id, false));
                app.toast(format!("♡ 已取消喜欢: {}", song.name));
            }
        }
        KeyCode::Char('r' | 'R') => {
            if matches!(app.state.navigation.page, Page::Main | Page::Lyrics) {
                app.reload_current_nav();
            }
        }
        KeyCode::Char('u' | 'U') if is_download_view(app) || is_local_music_view(app) => {
            let sel = app.state.navigation.content_selected;
            app.state
                .events
                .send(NavigationEvent::UploadCachedSong(sel));
        }
        KeyCode::Char('+' | '=') => {
            app.adjust_volume(0.05);
        }
        KeyCode::Char('-' | '_') => {
            app.adjust_volume(-0.05);
        }
        KeyCode::Char('z' | 'Z') => {
            app.cycle_nav_position();
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn handle_main_mouse(app: &mut App, kind: MouseEventKind, col: u16, row: u16) {
    if kind == MouseEventKind::Down(MouseButton::Left) {
        handle_click(app, col, row);
        return;
    }

    // Volume scroll: if mouse is over playerbar area
    let area = app.state.playerbar_area;
    if row >= area.y && row < area.y + area.height && col >= area.x && col < area.x + area.width {
        let vol = app.playback.state.volume;
        match kind {
            MouseEventKind::ScrollUp => {
                let new = (vol + 0.05).clamp(0.0, 1.0);
                app.playback.set_volume(new);
                app.toast(format!(" {}  {:.0}%", symbols().volume_high, new * 100.0));
            }
            MouseEventKind::ScrollDown => {
                let new = (vol - 0.05).clamp(0.0, 1.0);
                app.playback.set_volume(new);
                app.toast(format!(" {}  {:.0}%", symbols().volume_high, new * 100.0));
            }
            _ => {}
        }
        return;
    }

    match app.state.navigation.page {
        Page::Lyrics => {
            if kind == MouseEventKind::ScrollUp {
                app.playback.seek_relative(-5.0);
            } else if kind == MouseEventKind::ScrollDown {
                app.playback.seek_relative(5.0);
            }
        }
        Page::Main => {
            if kind == MouseEventKind::ScrollUp {
                content_select_prev(app);
            } else if kind == MouseEventKind::ScrollDown {
                content_select_next(app);
            }
        }
        Page::Playlist => {
            if kind == MouseEventKind::ScrollUp {
                playlist_select_prev(app);
            } else if kind == MouseEventKind::ScrollDown {
                playlist_select_next(app);
            }
        }
        _ => {}
    }
}

/// Left click: seek on the progress bar, toggle what the player bar shows, switch to a
/// navigation item, and select rows — clicking the row that is already selected opens it,
/// which is the mouse's Enter.
fn handle_click(app: &mut App, col: u16, row: u16) {
    if let Some(fraction) = hit::progress_fraction(app.state.gauge_area, col) {
        app.playback.seek_to_fraction(fraction);
        return;
    }

    if click_playerbar(app, col, row) {
        return;
    }

    if let Some((section, item)) = hit::nav_item(&app.state.navigation.nav.nav_hits, col, row)
        && app
            .state
            .navigation
            .nav
            .sections
            .get(section)
            .is_some_and(|s| item < s.items.len())
    {
        let nav = &mut app.state.navigation.nav;
        nav.focus_section = section;
        nav.section_states[section].select(Some(item));
        emit_nav_select(app);
        return;
    }

    match app.state.navigation.page {
        Page::Playlist => click_queue_page(app, col, row),
        Page::Main => click_content(app, col, row),
        _ => {}
    }
}

/// The player bar's own click targets: the spectrum row, the volume icon and the cover.
/// Returns whether the click was one of them.
fn click_playerbar(app: &mut App, col: u16, row: u16) -> bool {
    if app.state.spectrum_row_area.width > 0 && app.config.playerbar.visible.visualizer {
        // Clicking a readout hides it; `:visualizer on` / `:pitch on` bring it back.
        let (bars, pitch) =
            playerbar::spectrum_row(app.state.spectrum_row_area, &app.config.playerbar.visible);
        if app.config.playerbar.visible.pitch && hit::contains(pitch, col, row) {
            app.config.playerbar.visible.pitch = false;
            app.toast("音高已隐藏（:pitch on 恢复）".to_string());
            return true;
        }
        if hit::contains(bars, col, row) {
            app.config.playerbar.visible.visualizer = false;
            app.toast("频谱已隐藏（:visualizer on 恢复）".to_string());
            return true;
        }
    }

    if hit::contains(app.state.volume_area, col, row) {
        if app.playback.state.volume > 0.0 {
            app.state.volume_before_mute = Some(app.playback.state.volume);
            app.playback.set_volume(0.0);
            app.toast(" 静音".to_string());
        } else {
            let restored = app.state.volume_before_mute.take().unwrap_or(0.65);
            app.playback.set_volume(restored);
            app.toast(format!(
                " {}  {:.0}%",
                symbols().volume_high,
                restored * 100.0
            ));
        }
        return true;
    }

    if hit::contains(app.state.cover_area, col, row) {
        // The cover is the "now playing" surface, so clicking it flips to the lyrics and
        // back, exactly like the `l` key.
        let next = match app.state.navigation.page {
            Page::Main => Page::Lyrics,
            Page::Lyrics => Page::Main,
            other => other,
        };
        app.state.events.send(NavigationEvent::Navigate(next));
        return true;
    }

    false
}

/// Queue page: tabs switch queues, and rows select, then play on the second click.
fn click_queue_page(app: &mut App, col: u16, row: u16) {
    let key = hit::queue_tab(&app.state.queue_hits.tabs, col, row).map(str::to_string);
    if let Some(key) = key {
        app.playback.activate_queue(&key);
        app.state.navigation.playlist_selected = app.playback.queue_current_index().unwrap_or(0);
        return;
    }

    let (table, offset) = (app.state.queue_hits.table, app.state.queue_hits.offset);
    let total = app.playback.queue_len();
    let Some(index) = hit::table_row(table, 1, offset, total, col, row) else {
        return;
    };

    let repeat = app.state.navigation.playlist_selected == index;
    app.state.navigation.playlist_selected = index;
    if repeat {
        playlist_play_selected(app);
    }
}

/// Content table of the main page: select, then open on the second click.
fn click_content(app: &mut App, col: u16, row: u16) {
    let total = content_item_count(app);
    let Some(index) = hit::table_row(
        app.state.content_inner,
        1,
        app.state.content_offset,
        total,
        col,
        row,
    ) else {
        return;
    };

    let repeat = app.state.navigation.content_selected == index;
    app.state.navigation.content_selected = index;
    app.state.navigation.table_state.select(Some(index));
    check_load_more(app, total);
    if repeat {
        if app.state.navigation.table_mode == TableMode::Cell {
            cell_enter_action(app);
        } else {
            row_enter_action(app);
        }
    }
}

fn current_api(app: &App) -> Option<&str> {
    app.state.navigation.nav.selected_api()
}

fn is_daily_recommend(app: &App) -> bool {
    current_api(app) == Some("recommend_songs")
}

fn is_download_view(app: &App) -> bool {
    current_api(app) == Some("download")
}

fn is_local_music_view(app: &App) -> bool {
    current_api(app) == Some("local_music")
}
