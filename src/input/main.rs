use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEventKind};
use ncm_api::SongInfo;
use ratatui::layout::Rect;
use std::sync::Arc;

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
    state::{ArtistIo, ContentState, Page, TableMode},
    text_input::TextInput,
    ui::playerbar,
};

pub(super) fn handle_main_key(app: &mut App, key_event: KeyEvent) -> color_eyre::Result<()> {
    match key_event.code {
        KeyCode::Esc => {
            if app.state.navigation.page == Page::Artist {
                // The artist page is not part of the content breadcrumb stack — it is opened
                // from a row rather than by walking the table — so leaving it is a page change,
                // not a restore, and there is no breadcrumb for `ContentRestore` to pop.
                app.state.events.send(NavigationEvent::Navigate(Page::Main));
            } else {
                app.state.events.send(NavigationEvent::ContentRestore);
            }
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
            if app.state.navigation.page == Page::Artist {
                app.state.navigation.artist.select_prev();
            } else if app.state.navigation.page == Page::Playlist {
                playlist_select_prev(app);
            } else {
                content_select_prev(app);
            }
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            if app.state.navigation.page == Page::Artist {
                app.state.navigation.artist.select_next();
            } else if app.state.navigation.page == Page::Playlist {
                playlist_select_next(app);
            } else {
                content_select_next(app);
            }
        }
        KeyCode::Char('g') => {
            if app.state.navigation.page == Page::Artist {
                app.state.navigation.artist.select_first();
            } else if app.state.navigation.page == Page::Playlist {
                playlist_select_first(app);
            } else {
                content_select_first(app);
            }
        }
        KeyCode::Char('G') => {
            if app.state.navigation.page == Page::Artist {
                app.state.navigation.artist.select_last();
            } else if app.state.navigation.page == Page::Playlist {
                playlist_select_last(app);
            } else {
                content_select_last(app);
            }
        }
        KeyCode::Enter => {
            if app.state.navigation.page == Page::Artist {
                // Enter plays the hot song under the page's cursor.
                artist_play_selected(app);
            } else if app.state.navigation.page == Page::Playlist {
                playlist_play_selected(app);
            } else if !open_artist_from_table(app) {
                // Not an artist row: the table's own Enter, cell mode or row mode.
                if app.state.navigation.table_mode == TableMode::Cell {
                    cell_enter_action(app);
                } else {
                    row_enter_action(app);
                }
            }
        }
        KeyCode::Left => {
            if app.state.navigation.table_mode == TableMode::Cell
                && matches!(
                    app.state.navigation.page,
                    Page::Main | Page::Lyrics
                )
            {
                cell_select_prev_column(app);
            } else if app.playback.current_song().is_some() {
                let interval = app.config.seek_interval_secs as f64;
                app.playback.seek_relative(-interval);
            }
        }
        KeyCode::Right => {
            if app.state.navigation.table_mode == TableMode::Cell
                && matches!(
                    app.state.navigation.page,
                    Page::Main | Page::Lyrics
                )
            {
                cell_select_next_column(app);
            } else if app.playback.current_song().is_some() {
                let interval = app.config.seek_interval_secs as f64;
                app.playback.seek_relative(interval);
            }
        }
        KeyCode::Char('l') => open_page_key(app, 'l'),
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
            // The queue opens on the song that is playing.
            if app.state.navigation.page == Page::Main {
                app.state.navigation.playlist_selected =
                    app.playback.queue_current_index().unwrap_or(0);
            }
            open_page_key(app, 'f');
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
        KeyCode::Char(' ') => toggle_play_pause(app),
        KeyCode::Char('m') => cycle_play_mode(app),
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
            if app.state.navigation.page == Page::Artist {
                reload_artist(app);
            } else if matches!(app.state.navigation.page, Page::Main | Page::Lyrics) {
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
        // `v`/`V` toggle the two audio readouts, exactly like `:visualizer` / `:pitch`.
        KeyCode::Char('v') => {
            let on = !app.config.playerbar.visible.visualizer;
            app.set_visualizer(on);
        }
        KeyCode::Char('V') => {
            let on = !app.config.playerbar.visible.pitch;
            app.set_pitch(on);
        }
        _ => {}
    }
    Ok(())
}

/// A page key — `l` for the lyrics, `f` for the queue: it opens the page whose table entry
/// names it, and that page's own key comes back to the main page. See [`Page::on_key`].
fn open_page_key(app: &mut App, key: char) {
    if let Some(next) = app.state.navigation.page.on_key(key) {
        app.state.events.send(NavigationEvent::Navigate(next));
    }
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
        Page::Artist => {
            if kind == MouseEventKind::ScrollUp {
                app.state.navigation.artist.select_prev();
            } else if kind == MouseEventKind::ScrollDown {
                app.state.navigation.artist.select_next();
            }
        }
        _ => {}
    }
}

/// A player bar button the mouse can land on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlayerbarTarget {
    Transport(playerbar::ControlButton),
    Mode,
    Like,
}

/// Which button a click lands on, if any: the transport buttons, the mode icon and the
/// like hearts are hit-tested against the rects the last frame published, so they follow
/// whatever the active layout drew.
fn playerbar_target(
    transport: &[(playerbar::ControlButton, Rect); 3],
    mode: Rect,
    likes: &[Rect; 2],
    col: u16,
    row: u16,
) -> Option<PlayerbarTarget> {
    if let Some((button, _)) = transport
        .iter()
        .find(|(_, rect)| hit::contains(*rect, col, row))
    {
        return Some(PlayerbarTarget::Transport(*button));
    }
    if hit::contains(mode, col, row) {
        return Some(PlayerbarTarget::Mode);
    }
    if likes.iter().any(|rect| hit::contains(*rect, col, row)) {
        return Some(PlayerbarTarget::Like);
    }
    None
}

/// Start or pause playback, the way `Space` and the play button both mean it.
fn toggle_play_pause(app: &mut App) {
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

/// Move to the next play mode, the way `m` and the mode icon both mean it.
fn cycle_play_mode(app: &mut App) {
    let mode = app.playback.cycle_mode();
    let (icon, label) = mode_icon(&mode);
    app.toast(format!("{icon} 循环: {label}"));
}

/// Like or unlike the current song, the way the heart in the player bar means it.
fn toggle_like(app: &mut App) {
    let Some(song) = app.playback.current_song() else {
        return;
    };
    let like = !app.playback.state.liked;
    app.state
        .events
        .send(PlaybackEvent::LikeSong(song.id, like));
    if like {
        app.toast(format!("♥  {}", song.name));
    } else {
        app.toast(format!("♡  {}", song.name));
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

/// The player bar's own click targets: the readouts, the transport buttons, the mode
/// icon, the like button, the volume icon and the cover. Returns whether the click was one
/// of them.
fn click_playerbar(app: &mut App, col: u16, row: u16) -> bool {
    if app.state.spectrum_row_area.width > 0 || app.state.pitch_area.width > 0 {
        // Clicking a readout hides it; `:visualizer on` / `:pitch on` bring it back. The
        // pitch cell is its own in layouts that have one, and part of the spectrum row
        // otherwise.
        let (bars, shared) =
            playerbar::spectrum_row(app.state.spectrum_row_area, &app.config.playerbar.visible);
        let pitch = if app.state.pitch_area.width > 0 {
            app.state.pitch_area
        } else {
            shared
        };

        if app.config.playerbar.visible.pitch && hit::contains(pitch, col, row) {
            app.config.playerbar.visible.pitch = false;
            app.toast("音高已隐藏（:pitch on 恢复）".to_string());
            return true;
        }
        if app.config.playerbar.visible.visualizer && hit::contains(bars, col, row) {
            app.config.playerbar.visible.visualizer = false;
            app.toast("频谱已隐藏（:visualizer on 恢复）".to_string());
            return true;
        }
    }

    let buttons = playerbar_target(
        &app.state.transport,
        app.state.mode_area,
        &app.state.like_areas,
        col,
        row,
    );
    if let Some(target) = buttons {
        match target {
            PlayerbarTarget::Transport(playerbar::ControlButton::Prev) => app.playback.prev(),
            PlayerbarTarget::Transport(playerbar::ControlButton::PlayPause) => {
                toggle_play_pause(app);
            }
            PlayerbarTarget::Transport(playerbar::ControlButton::Next) => app.playback.next(),
            PlayerbarTarget::Mode => cycle_play_mode(app),
            PlayerbarTarget::Like => toggle_like(app),
        }
        return true;
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
    // The second click opens the row, which for an artist row is the artist page — the same
    // thing Enter does.
    if repeat && !open_artist_from_table(app) {
        if app.state.navigation.table_mode == TableMode::Cell {
            cell_enter_action(app);
        } else {
            row_enter_action(app);
        }
    }
}

/// The artist row under the cursor, if the table is showing artists: the id, and the name and
/// portrait the row already carries.
fn selected_artist(app: &App) -> Option<(u64, String, String)> {
    let ContentState::Singers(singers) = app.state.navigation.content.as_ref() else {
        return None;
    };
    let singer = singers.get(app.state.navigation.content_selected)?;
    // The API sends id 0 for rows it has no artist for; such a row has nothing to open.
    (singer.id != 0).then(|| (singer.id, singer.name.clone(), singer.pic_url.clone()))
}

/// Open the artist page on the row under the cursor, and say whether the table had one.
///
/// The hot-artists table is the only way into the page, so this is where the table's rows stop
/// being rows and start being artists: the row's name and portrait go along, because they are
/// what the page can draw before its own request lands. Everything else about Enter is
/// unchanged — an artist row opens the page, any other row (or cell) keeps its old meaning.
fn open_artist_from_table(app: &mut App) -> bool {
    let Some((id, name, pic_url)) = selected_artist(app) else {
        return false;
    };
    let io = artist_io(app);
    app.state.navigation.artist.open(id, name, pic_url, io);
    app.state.events.send(NavigationEvent::Navigate(Page::Artist));
    true
}

/// Load the artist the page is showing again (`r`). The page keeps its header, so this is the
/// retry a failed page offers.
fn reload_artist(app: &mut App) {
    let io = artist_io(app);
    app.state.navigation.artist.reload(io);
}

/// Where the artist page gets its data: the API client, the proxy-aware image client, and the
/// terminal's image protocol for the portrait.
fn artist_io(app: &App) -> ArtistIo {
    ArtistIo {
        client: app.service.client().clone(),
        http: app.cover_http.clone(),
        picker: app.picker.clone(),
        repaint: app.state.events.sender(),
    }
}

/// Play the hot song under the artist page's cursor.
///
/// The page's songs live in the page rather than in the content table, and
/// `PlaybackEvent::SongPlay` resolves a song against the open table — it would find nothing
/// here — so the queue is built from the page's own list.
fn artist_play_selected(app: &mut App) {
    let (key, index) = {
        let artist = &app.state.navigation.artist;
        let songs = artist.hot_songs();
        if songs.is_empty() {
            return;
        }
        // The same context string the artist's songs use as their breadcrumb on the main page,
        // so playing from here lands in one queue per artist rather than a new one each time.
        (
            format!("歌手: {}", artist.name),
            artist.song_selected.min(songs.len() - 1),
        )
    };

    let songs: Vec<Arc<SongInfo>> = app
        .state
        .navigation
        .artist
        .hot_songs()
        .iter()
        .cloned()
        .map(Arc::new)
        .collect();
    app.playback.play_songs(&key, songs, index);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Enter on the hot-artists table reads the row as an artist: the id is what the page is
    /// opened for, and the name and portrait travel with it so the page has a header before
    /// its own request lands. Nothing else in the app's tables is an artist.
    #[tokio::test]
    async fn only_artist_rows_offer_an_artist_to_open() {
        use crate::{config::Config, state::ContentState};
        use ncm_api::SingerInfo;

        // Building the app builds its HTTP clients, which need the process-wide crypto
        // provider first — the same line `src/main.rs` runs at startup.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut app = App::new(Config::default(), false).expect("app");
        app.state.navigation.content = ContentState::Singers(vec![
            SingerInfo {
                id: 6452,
                name: "周杰伦".into(),
                pic_url: "https://p3.music.126.net/portrait.jpg".into(),
            },
            // The API sends id 0 for a row it has no artist for.
            SingerInfo {
                id: 0,
                name: "未知歌手".into(),
                pic_url: String::new(),
            },
        ])
        .into();

        app.state.navigation.content_selected = 0;
        assert_eq!(
            selected_artist(&app),
            Some((
                6452,
                "周杰伦".to_string(),
                "https://p3.music.126.net/portrait.jpg".to_string()
            ))
        );

        app.state.navigation.content_selected = 1;
        assert_eq!(selected_artist(&app), None);

        // A song table's rows are not artists, however they are indexed.
        app.state.navigation.content = ContentState::Songs(Vec::new()).into();
        app.state.navigation.content_selected = 0;
        assert_eq!(selected_artist(&app), None);
    }

    /// A click has to map to the button that is drawn under it, for both controls row
    /// alignments, for the mode cell and for the hearts — the mapping is what the mouse
    /// handlers act on, so it is worth pinning down without a terminal.
    #[test]
    fn clicks_map_to_the_button_under_the_cursor() {
        let area = Rect::new(0, 4, 60, 1);
        let mode = Rect::new(58, 4, 1, 1);
        let likes = [Rect::new(2, 6, 1, 1), Rect::new(2, 7, 1, 1)];

        for centered in [true, false] {
            let transport = playerbar::control_rects(area, centered);
            let target = |rect: Rect| playerbar_target(&transport, mode, &likes, rect.x, rect.y);

            assert_eq!(
                target(transport[0].1),
                Some(PlayerbarTarget::Transport(playerbar::ControlButton::Prev)),
                "centered={centered}"
            );
            assert_eq!(
                target(transport[1].1),
                Some(PlayerbarTarget::Transport(
                    playerbar::ControlButton::PlayPause
                )),
                "centered={centered}"
            );
            assert_eq!(
                target(transport[2].1),
                Some(PlayerbarTarget::Transport(playerbar::ControlButton::Next)),
                "centered={centered}"
            );

            // The cell just past the last icon, and the gap between two of them, are dead.
            let last = transport[2].1;
            assert_eq!(target(Rect::new(last.right(), last.y, 1, 1)), None);
            let first = transport[0].1;
            assert_eq!(
                target(Rect::new(first.right(), first.y, 1, 1)),
                None,
                "the gap"
            );
        }

        // No transport buttons on screen, but the mode cell and both hearts still are.
        let none = [(playerbar::ControlButton::Prev, Rect::default()); 3];
        assert_eq!(
            playerbar_target(&none, mode, &likes, 58, 4),
            Some(PlayerbarTarget::Mode)
        );
        for heart in likes {
            assert_eq!(
                playerbar_target(&none, Rect::default(), &likes, heart.x, heart.y),
                Some(PlayerbarTarget::Like)
            );
        }
        assert_eq!(
            playerbar_target(&none, Rect::default(), &[Rect::default(); 2], 30, 20),
            None,
            "empty space is not a button"
        );
    }
}
