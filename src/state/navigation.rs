use std::{cell::RefCell, sync::Arc};

use ncm_api::{ArtistAlbum, ArtistDetail, LoginInfo, NcmClient, SongInfo};
use ratatui::{
    layout::Rect,
    widgets::{ListState, TableState},
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender, error::TryRecvError};

use super::{
    Page, PaginationInfo,
    content::{ContentState, TableMode},
    search::SearchState,
};
pub use crate::config::{NavItemConfig, NavSectionConfig as NavSection};
use crate::{
    config::NavConfig,
    event::{AppEvent, Event},
};

pub struct NavState {
    pub sections: Vec<NavSection>,
    pub section_states: Vec<ListState>,
    pub focus_section: usize,
    pub subtitle: Option<String>,
    /// Horizontal scroll offset (in cells) for the top navigation mode.
    pub scroll_x: u16,
    /// Where each currently visible item was drawn (`section`, `item`, screen area),
    /// refreshed by the draw pass and read by mouse input to map a click back to an item.
    pub nav_hits: Vec<(usize, usize, Rect)>,
}

impl NavState {
    pub fn from_config(config: &NavConfig) -> Self {
        let sections: Vec<NavSection> = if config.sections.is_empty() {
            NavConfig::default().sections
        } else {
            config.sections.clone()
        };

        let section_states: Vec<ListState> = sections
            .iter()
            .map(|s| {
                let mut state = ListState::default();
                if !s.items.is_empty() {
                    state.select(Some(0));
                }
                state
            })
            .collect();

        Self {
            sections,
            section_states,
            focus_section: 0,
            subtitle: None,
            scroll_x: 0,
            nav_hits: Vec::new(),
        }
    }

    pub fn restore_focus_by_api(&mut self, api: &str) {
        for (s, section) in self.sections.iter().enumerate() {
            if let Some(i) = section
                .items
                .iter()
                .position(|item| item.api.as_deref() == Some(api))
            {
                self.focus_section = s;
                self.section_states[s].select(Some(i));
                break;
            }
        }
    }

    /// The focused section, if the focus index is in range.
    pub fn focused_section(&self) -> Option<&NavSection> {
        self.sections.get(self.focus_section)
    }

    /// Selected item index within the focused section.
    pub fn selected_index(&self) -> Option<usize> {
        self.section_states
            .get(self.focus_section)
            .and_then(|st| st.selected())
    }

    /// The currently focused and selected nav item, if any.
    pub fn selected_item(&self) -> Option<&NavItemConfig> {
        let section = self.focused_section()?;
        self.selected_index().and_then(|i| section.items.get(i))
    }

    /// The api of the currently focused nav item, if any.
    pub fn selected_api(&self) -> Option<&str> {
        self.selected_item().and_then(|item| item.api.as_deref())
    }

    /// The display name of the currently focused nav item, if any.
    pub fn selected_name(&self) -> Option<&str> {
        self.selected_item().map(|item| item.name.as_str())
    }
}

/// (rendered title, focus_section, selected_index, generation, content item count)
type TitleCache = (Arc<String>, usize, Option<usize>, u64, usize);

#[derive(Clone)]
pub struct BreadcrumbEntry {
    pub content: Arc<ContentState>,
    pub api: Option<String>,
    pub subtitle: Option<String>,
    pub content_selected: usize,
    pub content_column_selected: usize,
    pub table_mode: TableMode,
    pub table_state: TableState,
}

pub struct NavigationState {
    pub page: Page,
    pub user: Option<LoginInfo>,
    pub nav: NavState,
    pub content: Arc<ContentState>,
    pub history: Vec<BreadcrumbEntry>,
    pub content_selected: usize,
    pub content_column_selected: usize,
    pub table_mode: TableMode,
    pub table_state: TableState,
    pub playlist_selected: usize,
    /// Horizontal scroll offset of the queue's tab bar (persisted across renders
    /// so the selected tab stays in view).
    pub queue_tab_scroll_x: u16,
    /// The artist page's own state: who it shows and what has loaded for them.
    pub artist: ArtistState,
    pub search: SearchState,
    pub pagination: Option<PaginationInfo>,
    pub generation: u64,
    /// True when the current `Songs` content is a search result (Enter plays
    /// only the selected song instead of appending the whole list to the queue).
    pub content_is_search: bool,
    /// Cached block title string, keyed by
    /// (focus_section, selected_index, generation, content item count).
    /// The item count is part of the key so incremental (paged) loads that
    /// append to the current content re-render the `{count}` placeholder.
    pub title_cache: RefCell<Option<TitleCache>>,
}

impl NavigationState {
    pub fn set_content(&mut self, content: ContentState) {
        self.content = Arc::new(content);
        self.content_selected = 0;
        self.content_column_selected = 0;
        self.table_mode = TableMode::Row;
        self.table_state = TableState::default();
        self.table_state.select_first();
        self.pagination = None;
        *self.title_cache.borrow_mut() = None;
    }

    pub fn push_breadcrumb(&mut self) {
        let api = self.nav.selected_api().map(str::to_string);
        self.history.push(BreadcrumbEntry {
            content: Arc::clone(&self.content),
            api,
            subtitle: self.nav.subtitle.clone(),
            content_selected: self.content_selected,
            content_column_selected: self.content_column_selected,
            table_mode: self.table_mode,
            table_state: self.table_state,
        });
    }

    pub fn pop_breadcrumb(&mut self) -> bool {
        if let Some(entry) = self.history.pop() {
            self.content = entry.content;
            self.content_selected = entry.content_selected;
            self.content_column_selected = entry.content_column_selected;
            self.table_mode = entry.table_mode;
            self.table_state = entry.table_state;
            self.nav.subtitle = entry.subtitle;
            if let Some(api) = &entry.api {
                self.nav.restore_focus_by_api(api);
            }
            *self.title_cache.borrow_mut() = None;
            true
        } else {
            false
        }
    }

    /// Remove a song from the active content while keeping selection and pagination valid.
    pub fn remove_song(&mut self, song_id: u64) -> bool {
        if !Arc::make_mut(&mut self.content).remove_song(song_id) {
            return false;
        }

        let len = self.content.len();
        self.content_selected = self.content_selected.min(len.saturating_sub(1));
        self.table_state
            .select((len > 0).then_some(self.content_selected));
        if let Some(pagination) = &mut self.pagination {
            pagination.total = pagination.total.saturating_sub(1);
        }
        *self.title_cache.borrow_mut() = None;
        true
    }

    /// Insert a song at the top of the current song content, mirroring a newly
    /// liked song onto the open "我喜欢的音乐" root. Returns whether an item was inserted.
    pub fn insert_song_at_top(&mut self, song: Arc<SongInfo>) -> bool {
        if !Arc::make_mut(&mut self.content).insert_song_at_top(song) {
            return false;
        }
        // Follow the previously selected row, which was shifted down by one.
        self.content_selected = self.content_selected.saturating_add(1);
        if self.table_state.selected().is_some() {
            self.table_state.select(Some(self.content_selected));
        }
        if let Some(pagination) = &mut self.pagination {
            pagination.total = pagination.total.saturating_add(1);
        }
        *self.title_cache.borrow_mut() = None;
        true
    }

    pub fn clear_breadcrumb(&mut self) {
        self.history.clear();
    }
}

// --- Artist page ---

/// How many albums the page asks for. The list is a summary pane rather than the album's own
/// page, and one API page of it is the whole pane.
const ARTIST_ALBUM_LIMIT: u16 = 50;

/// Edge of the portrait request, in pixels. The API resizes server-side (`?param=`), and the
/// pane is a handful of rows tall, so a thumbnail is all it can show.
const ARTIST_PORTRAIT_PIXELS: u32 = 240;

/// What the artist page needs from the app to reach the network: the API client, the image
/// client (the one that honours the proxy config) and the terminal's image protocol.
#[derive(Clone)]
pub struct ArtistIo {
    pub client: Arc<NcmClient>,
    pub http: reqwest::Client,
    pub picker: Picker,
    /// How the page asks for the frame to be drawn again.
    ///
    /// A load reports on the page's own channel, which only the draw pass drains, and the main
    /// loop draws after an event and otherwise waits for one — with nothing playing, a load that
    /// finished would sit in the channel until the reader pressed a key. `AppEvent::Repaint` is
    /// exactly that wake-up: it changes nothing and only gets the frame drawn.
    pub repaint: UnboundedSender<Event>,
}

/// What the artist page has to draw.
pub enum ArtistData {
    /// The profile request is out.
    Loading,
    /// Profile and hot songs are in. The album list is a second request: when it fails the
    /// page keeps everything else and only the album pane reports the failure.
    Ready {
        detail: ArtistDetail,
        albums: Result<Vec<ArtistAlbum>, String>,
    },
    /// The profile request itself failed. `r` reloads it and `Esc` leaves the page, so a
    /// failure never strands the reader here.
    Failed(String),
}

/// The artist page's own state.
///
/// The page keeps its own loader instead of going through `ApiService` and `ContentState`:
/// an artist profile is not table content — it must not replace what the main page is
/// showing, and it never joins the breadcrumb stack (the page is opened and left by name).
pub struct ArtistState {
    /// Artist the page is showing.
    pub id: u64,
    /// Name and portrait taken from the list row that opened the page. They are already
    /// known, so the header is real before the profile request lands.
    pub name: String,
    pub pic_url: String,
    pub data: ArtistData,
    /// Cursor over the hot songs of a loaded profile.
    pub song_selected: usize,
    /// The portrait decoded for the terminal's image protocol. `None` until it arrives, and
    /// it stays `None` when the download or the decoding failed — the page reads fine
    /// without a portrait, so that case has no error of its own.
    pub avatar: Option<StatefulProtocol>,
    /// What the running load reported. A profile belongs to this page rather than to the app,
    /// so it arrives on the page's own channel instead of as an app event; the app hears about
    /// the load only through `ArtistIo::repaint`, which asks for a frame.
    rx: Option<UnboundedReceiver<ArtistMsg>>,
}

/// What a running load reports back to the page.
enum ArtistMsg {
    Ready {
        detail: ArtistDetail,
        albums: Result<Vec<ArtistAlbum>, String>,
    },
    Failed(String),
    Avatar(StatefulProtocol),
}

impl Default for ArtistState {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            pic_url: String::new(),
            data: ArtistData::Loading,
            song_selected: 0,
            avatar: None,
            rx: None,
        }
    }
}

impl ArtistState {
    /// Show one artist and load its profile. `name` and `pic_url` come from the list row
    /// that opened the page and only have to carry the header until the profile arrives.
    pub fn open(&mut self, id: u64, name: String, pic_url: String, io: ArtistIo) {
        self.id = id;
        self.name = name;
        self.pic_url = pic_url;
        self.load(io);
    }

    /// Load the artist again, for a failed page (`r`). Nothing to retry before the first open.
    pub fn reload(&mut self, io: ArtistIo) {
        if self.id != 0 {
            self.load(io);
        }
    }

    fn load(&mut self, io: ArtistIo) {
        self.data = ArtistData::Loading;
        self.song_selected = 0;
        self.avatar = None;

        let (tx, rx) = mpsc::unbounded_channel();
        self.rx = Some(rx);
        let id = self.id;

        tokio::spawn(async move {
            let detail = match io.client.artist_detail(id).await {
                Ok(detail) => detail,
                Err(e) => {
                    let _ = tx.send(ArtistMsg::Failed(e.to_string()));
                    // A failure has to reach the screen too, and it is the same story: with
                    // nothing else happening, only a wake-up gets the frame drawn.
                    let _ = io.repaint.send(AppEvent::Repaint.into());
                    return;
                }
            };
            let albums = io
                .client
                .artist_albums(id, 0, ARTIST_ALBUM_LIMIT)
                .await
                .map_err(|e| e.to_string());

            // The text goes out first: the portrait is a second download, and the page is
            // usable — and already worth reading — without it.
            let portrait = detail.pic_url.clone();
            let _ = tx.send(ArtistMsg::Ready { detail, albums });
            let _ = io.repaint.send(AppEvent::Repaint.into());
            if let Some(protocol) = load_portrait(&io, &portrait).await {
                let _ = tx.send(ArtistMsg::Avatar(protocol));
                let _ = io.repaint.send(AppEvent::Repaint.into());
            }
        });
    }

    /// Take in whatever the loader has sent since the last frame.
    ///
    /// The draw pass calls this — and the load itself asks for that frame with
    /// `AppEvent::Repaint`, so a load that finishes while nothing else is happening still
    /// reaches the screen instead of waiting for the next keypress.
    pub fn poll(&mut self) {
        let Some(mut rx) = self.rx.take() else {
            return;
        };
        // The receiver is a local while the messages are applied, so `apply` can borrow the
        // page mutably.
        loop {
            match rx.try_recv() {
                Ok(msg) => self.apply(msg),
                Err(TryRecvError::Empty) => break,
                // The worker is gone: this channel can produce nothing else.
                Err(TryRecvError::Disconnected) => return,
            }
        }
        self.rx = Some(rx);
    }

    fn apply(&mut self, msg: ArtistMsg) {
        match msg {
            ArtistMsg::Ready { detail, albums } => {
                // The profile is the better source for both: the row's copy of them can be
                // stale, and the artist may have been renamed since the list was fetched.
                self.name = detail.name.clone();
                if !detail.pic_url.is_empty() {
                    self.pic_url = detail.pic_url.clone();
                }
                self.data = ArtistData::Ready { detail, albums };
            }
            ArtistMsg::Failed(error) => self.data = ArtistData::Failed(error),
            ArtistMsg::Avatar(protocol) => self.avatar = Some(protocol),
        }
    }

    /// The hot songs of the loaded profile; empty while loading and after a failure.
    pub fn hot_songs(&self) -> &[SongInfo] {
        match &self.data {
            ArtistData::Ready { detail, .. } => &detail.hot_songs,
            ArtistData::Loading | ArtistData::Failed(_) => &[],
        }
    }

    /// The song the cursor is on.
    pub fn selected_song(&self) -> Option<&SongInfo> {
        self.hot_songs().get(self.song_selected)
    }

    /// Move the cursor down the hot songs, wrapping the way the main table's does.
    pub fn select_next(&mut self) {
        let count = self.hot_songs().len();
        if count > 0 {
            self.song_selected = (self.song_selected + 1) % count;
        }
    }

    /// Move the cursor up, wrapping.
    pub fn select_prev(&mut self) {
        let count = self.hot_songs().len();
        if count > 0 {
            self.song_selected = (self.song_selected + count - 1) % count;
        }
    }

    pub fn select_first(&mut self) {
        self.song_selected = 0;
    }

    pub fn select_last(&mut self) {
        self.song_selected = self.hot_songs().len().saturating_sub(1);
    }
}

/// Download the artist portrait and decode it for the terminal's image protocol. Every
/// failure returns `None`: the portrait is decoration, and the page says what it has either
/// way.
async fn load_portrait(io: &ArtistIo, url: &str) -> Option<StatefulProtocol> {
    if url.is_empty() {
        return None;
    }
    // `?param=` is the API's own resize parameter, the one `NcmClient::download_img` uses.
    let url = format!("{url}?param={ARTIST_PORTRAIT_PIXELS}y{ARTIST_PORTRAIT_PIXELS}");
    let bytes = io.http.get(url).send().await.ok()?.bytes().await.ok()?;
    let picker = io.picker.clone();

    // Decoding is CPU work on a byte buffer; the resize itself happens at draw time.
    tokio::task::spawn_blocking(move || {
        let image = image::load_from_memory(&bytes).ok()?;
        Some(picker.new_resize_protocol(image))
    })
    .await
    .ok()?
}
