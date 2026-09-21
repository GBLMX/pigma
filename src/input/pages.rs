//! The pages' own keys: the artist page's and the queue's, each a layer of its own.
//!
//! What `↑`, `Enter` or `/` mean depends on the page that is up — on the queue page `↑` walks the
//! queue, on the artist page it walks that artist's songs, and everywhere else it walks the table.
//! Those used to be `if page == X` arms inside `input::main`'s key map; here each page owns them,
//! which is the layering Yazi's keymap has (`Keymap::chords(layer)`) and the shape ratatui's own
//! component guidance describes (`handle_key_events` on the component). [`crate::state::page::PageSpec::keys`]
//! is what hands a layer back to the key map, which asks it before the global keys.

use crossterm::event::{KeyCode, KeyEvent};

use super::{
    content::{
        playlist_play_selected, playlist_select_first, playlist_select_last, playlist_select_next,
        playlist_select_prev,
    },
    main::{artist_activate, reload_artist},
};
use crate::{app::App, event::NavigationEvent, state::Page, text_input::TextInput};

/// The artist page's keys.
///
/// `Esc` leaves the page the way any page is left, its own two lists are walked with the same
/// keys the tables use, `Tab` decides which of them the walking happens in, and `Enter` acts on
/// what the cursor is on: the hot song is played, the album is opened.
pub(crate) fn artist_keys(app: &mut App, key_event: KeyEvent) -> bool {
    match key_event.code {
        // The artist page is not part of the content breadcrumb stack — it is opened from a row
        // rather than by walking the table — so leaving it is a page change, not a restore, and
        // there is no breadcrumb for `ContentRestore` to pop.
        KeyCode::Esc => app.state.events.send(NavigationEvent::Navigate(Page::Main)),
        // There are two lists and one cursor, so `Tab` and `Shift+Tab` do the same thing: they
        // hand the cursor to the other list. `←`/`→` are not used for it — the app's seek
        // bindings own those, and a page that quietly re-bound them would be a trap.
        KeyCode::Tab | KeyCode::BackTab => app.state.navigation.artist.toggle_focus(),
        KeyCode::Up | KeyCode::Char('k' | 'K') => app.state.navigation.artist.select_prev(),
        KeyCode::Down | KeyCode::Char('j' | 'J') => app.state.navigation.artist.select_next(),
        KeyCode::Char('g') => app.state.navigation.artist.select_first(),
        KeyCode::Char('G') => app.state.navigation.artist.select_last(),
        KeyCode::Enter => artist_activate(app),
        KeyCode::Char('r' | 'R') => reload_artist(app),
        _ => return false,
    }

    true
}

/// The queue's keys: its tabs, its rows, and the search that filters it.
pub(crate) fn playlist_keys(app: &mut App, key_event: KeyEvent) -> bool {
    match key_event.code {
        KeyCode::Tab => switch_queue(app, true),
        KeyCode::BackTab => switch_queue(app, false),
        KeyCode::Up | KeyCode::Char('k' | 'K') => playlist_select_prev(app),
        KeyCode::Down | KeyCode::Char('j' | 'J') => playlist_select_next(app),
        KeyCode::Char('g') => playlist_select_first(app),
        KeyCode::Char('G') => playlist_select_last(app),
        KeyCode::Enter => playlist_play_selected(app),
        KeyCode::Char('/') => open_queue_search(app),
        _ => return false,
    }

    true
}

/// Switch to the next (`forward`) or previous queue tab, keeping the cursor on the song that is
/// playing.
fn switch_queue(app: &mut App, forward: bool) {
    if let Some(key) = app.playback.switch_queue(forward) {
        app.state.navigation.playlist_selected = app.playback.queue_current_index().unwrap_or(0);
        app.toast(format!("▣ 队列: {key}"));
    }
}

/// `/` on the queue page filters the queue rather than the current table: the queue is what is on
/// screen, and it is not in the navigation table the app's search otherwise reads.
fn open_queue_search(app: &mut App) {
    let search = &mut app.state.navigation.search;
    search.filter_queue_only = true;
    search.unfiltered_songs = Some(app.playback.queue_songs().to_vec());

    search.input = TextInput::new();
    search.active = true;
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;
    use crate::{config::Config, input::main::handle_main_key};

    fn press(app: &mut App, key: KeyCode) {
        handle_main_key(app, KeyEvent::new(key, KeyModifiers::NONE)).expect("key");
    }

    fn app(page: Page) -> App {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut app = App::new(Config::default(), false).expect("app");
        app.state.navigation.page = page;

        app
    }

    /// The queue page's layer is what makes `↑` walk the queue instead of the table behind it, and
    /// `Tab` is the queue's own (the global `Tab` moves between navigation sections).
    #[tokio::test]
    async fn the_queue_page_walks_its_own_rows() {
        let mut app = app(Page::Playlist);
        app.playback
            .set_queue_songs((1..=3).map(|id| std::sync::Arc::new(song(id))).collect());

        press(&mut app, KeyCode::Char('j'));
        assert_eq!(
            app.state.navigation.playlist_selected, 1,
            "`j` walks the queue"
        );
        press(&mut app, KeyCode::Char('G'));
        assert_eq!(app.state.navigation.playlist_selected, 2);
        press(&mut app, KeyCode::Char('g'));
        assert_eq!(app.state.navigation.playlist_selected, 0);
    }

    /// The artist page keeps `Esc` to itself: it is not in the breadcrumb stack, so leaving it is
    /// a page change rather than a restore.
    #[tokio::test]
    async fn esc_leaves_the_artist_page() {
        let mut app = app(Page::Artist);
        let page = app.state.navigation.page;
        assert_eq!(page, Page::Artist);

        press(&mut app, KeyCode::Esc);

        // The page travels as a navigation event, so the app has to take the loop's event step.
        app.handle_events().await.expect("events");
        assert_eq!(app.state.navigation.page, Page::Main);
    }

    /// A key the page does not own falls through to the global map: on the main page `Tab` is
    /// the navigation key map's, because no page of its own is up to claim it. (The artist and
    /// the queue pages do claim it, which is what the page layer is for.)
    #[tokio::test]
    async fn a_key_the_page_does_not_own_falls_through() {
        let mut app = app(Page::Main);
        let Some(before) = app.state.navigation.nav.selected_index() else {
            // A navigation config with nothing in it is not this test's subject.
            return;
        };
        let items: usize = app
            .config
            .navigation
            .sections
            .iter()
            .map(|section| section.items.len())
            .sum();
        if items < 2 {
            return;
        }

        press(&mut app, KeyCode::Tab);

        assert_ne!(
            app.state.navigation.nav.selected_index(),
            Some(before),
            "`Tab` still moves through the navigation"
        );
    }

    fn song(id: u64) -> ncm_api::SongInfo {
        ncm_api::SongInfo {
            id,
            name: format!("song{id}"),
            singer: String::new(),
            artist_id: 0,
            album: String::new(),
            album_id: 0,
            pic_url: String::new(),
            duration: 60_000,
            mv: 0,
            copyright: ncm_api::SongCopyright::Free,
            local_path: None,
        }
    }
    /// An artist page with a profile in: two hot songs and two albums — the smallest lists that
    /// can tell a cursor that moved from one that did not.
    fn artist_with_data(app: &mut App) {
        let detail = ncm_api::ArtistDetail {
            id: 7,
            name: "阿七".into(),
            alias: Vec::new(),
            brief_desc: String::new(),
            pic_url: String::new(),
            album_size: 2,
            music_size: 2,
            hot_songs: vec![song(1), song(2)],
        };
        let albums = vec![
            ncm_api::ArtistAlbum {
                id: 11,
                name: "album-a".into(),
                pic_url: String::new(),
                size: 10,
                publish_time: 0,
            },
            ncm_api::ArtistAlbum {
                id: 12,
                name: "album-b".into(),
                pic_url: String::new(),
                size: 20,
                publish_time: 0,
            },
        ];
        app.state.navigation.artist.data = crate::state::ArtistData::Ready {
            detail,
            albums: Ok(albums),
        };
    }

    /// `Tab` is the artist page's own key: it hands the cursor between the hot songs and the
    /// albums, and each list remembers where it was, so switching back resumes rather than
    /// restarts.
    #[tokio::test]
    async fn tab_hands_the_cursor_to_the_other_pane() {
        let mut app = app(Page::Artist);
        artist_with_data(&mut app);

        press(&mut app, KeyCode::Char('j'));
        assert_eq!(
            app.state.navigation.artist.song_selected, 1,
            "`j` walks the songs"
        );

        press(&mut app, KeyCode::Tab);
        assert_eq!(
            app.state.navigation.artist.pane,
            crate::state::ArtistPane::Albums
        );
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(
            app.state.navigation.artist.album_selected, 1,
            "`j` walks the albums now"
        );
        assert_eq!(
            app.state.navigation.artist.song_selected, 1,
            "the songs kept their own cursor"
        );

        press(&mut app, KeyCode::BackTab);
        assert_eq!(
            app.state.navigation.artist.pane,
            crate::state::ArtistPane::Songs
        );
        press(&mut app, KeyCode::Char('k'));
        assert_eq!(
            app.state.navigation.artist.song_selected, 0,
            "and the songs kept their own walking"
        );
    }

    /// `Enter` on an album opens it as content, and `Esc` there goes back to the artist rather
    /// than to the main table: the artist page is not part of the breadcrumb stack, so opening
    /// an album leaves a return target behind instead of a breadcrumb.
    #[tokio::test]
    async fn enter_on_an_album_opens_it_and_esc_returns_to_the_artist() {
        let mut app = app(Page::Artist);
        artist_with_data(&mut app);

        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);
        app.handle_events().await.expect("events");

        assert_eq!(
            app.state.navigation.page,
            Page::Main,
            "an album is table content"
        );
        assert_eq!(app.state.navigation.return_page, Some(Page::Artist));
        assert_eq!(
            app.state.navigation.nav.subtitle.as_deref(),
            Some("album-b"),
            "the page says which album it is"
        );

        press(&mut app, KeyCode::Esc);
        app.handle_events().await.expect("events");
        assert_eq!(
            app.state.navigation.page,
            Page::Artist,
            "`Esc` came back to the artist"
        );
        assert_eq!(
            app.state.navigation.return_page, None,
            "and the way back was spent, not left for the next `Esc`"
        );
    }

    /// The artist page answers the mouse the way it answers the keys: a click picks the row it
    /// lands on in whichever pane it lands in — so it moves the focus too — and the wheel walks
    /// the pane it is over.
    #[tokio::test]
    async fn the_artist_page_answers_the_mouse() {
        use crossterm::event::{MouseButton, MouseEventKind};

        let mut app = app(Page::Artist);
        artist_with_data(&mut app);

        // The hit areas come from a frame, and the page is drawn inside the shell, so draw one
        // the way the app does.
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("backend");
        terminal
            .draw(|f| crate::ui::draw(f, &mut app))
            .expect("draw");

        let albums = app.state.artist_hits.albums;
        assert!(albums.height > 1, "the frame drew the album pane");
        // The first body row is the table's header, so the second album sits one row below it.
        let (x, y) = (albums.x + 1, albums.y + 2);
        crate::input::handle_mouse_event(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);

        assert_eq!(
            app.state.navigation.artist.pane,
            crate::state::ArtistPane::Albums,
            "clicking a list works in it"
        );
        assert_eq!(app.state.navigation.artist.album_selected, 1);

        crate::input::handle_mouse_event(&mut app, MouseEventKind::ScrollUp, x, y);
        assert_eq!(
            app.state.navigation.artist.album_selected, 0,
            "the wheel walked the pane it was over"
        );
    }

    /// A long album list scrolls, and what scrolled is what a click lands on: the window the
    /// frame drew and the first index the click handler uses are the same window.
    #[tokio::test]
    async fn the_album_pane_scrolls_and_still_clicks_what_it_shows() {
        use crossterm::event::{MouseButton, MouseEventKind};

        let mut app = app(Page::Artist);
        artist_with_data(&mut app);
        if let crate::state::ArtistData::Ready { albums, .. } =
            &mut app.state.navigation.artist.data
        {
            *albums = Ok((1..=30)
                .map(|id| ncm_api::ArtistAlbum {
                    id,
                    name: format!("album-{id}"),
                    pic_url: String::new(),
                    size: 1,
                    publish_time: 0,
                })
                .collect());
        }

        // Walk to the end of the albums, then draw: the frame has to have scrolled to show it.
        app.state.navigation.artist.pane = crate::state::ArtistPane::Albums;
        app.state.navigation.artist.select_last();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("backend");
        terminal
            .draw(|f| crate::ui::draw(f, &mut app))
            .expect("draw");

        let albums = app.state.artist_hits.albums;
        let offset = app.state.artist_hits.albums_offset;
        assert!(offset > 0, "a 30-album list in a short pane scrolled");
        assert!(
            albums.height > 1,
            "the pane still has rows: {albums:?} at offset {offset}"
        );

        // The top visible body row is the first album of the window, not the first album.
        crate::input::handle_mouse_event(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            albums.x + 1,
            albums.y + 1,
        );
        assert_eq!(
            app.state.navigation.artist.album_selected, offset,
            "the click landed on what the frame drew there"
        );
    }
}
