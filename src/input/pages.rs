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
    main::{artist_play_selected, reload_artist},
};
use crate::{app::App, event::NavigationEvent, state::Page, text_input::TextInput};

/// The artist page's keys.
///
/// `Esc` leaves the page the way any page is left, its own rows are walked with the same keys the
/// tables use, and `Enter` plays the hot song under the cursor.
pub(crate) fn artist_keys(app: &mut App, key_event: KeyEvent) -> bool {
    match key_event.code {
        // The artist page is not part of the content breadcrumb stack — it is opened from a row
        // rather than by walking the table — so leaving it is a page change, not a restore, and
        // there is no breadcrumb for `ContentRestore` to pop.
        KeyCode::Esc => app.state.events.send(NavigationEvent::Navigate(Page::Main)),
        KeyCode::Up | KeyCode::Char('k' | 'K') => app.state.navigation.artist.select_prev(),
        KeyCode::Down | KeyCode::Char('j' | 'J') => app.state.navigation.artist.select_next(),
        KeyCode::Char('g') => app.state.navigation.artist.select_first(),
        KeyCode::Char('G') => app.state.navigation.artist.select_last(),
        KeyCode::Enter => artist_play_selected(app),
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

    /// A key the page does not own falls through to the global map: on the artist page `Tab` is
    /// still the navigation key map's, not the page's.
    #[tokio::test]
    async fn a_key_the_page_does_not_own_falls_through() {
        let mut app = app(Page::Artist);
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
}
