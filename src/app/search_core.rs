//! Shared search core used by the TUI search bar and the IPC `search` request.
//!
//! The TUI search is async-fire-and-forget (spawns a task, pushes
//! [`crate::event::NavigationEvent`]s, updates navigation state) while the
//! IPC server must answer `boxpigma msg search` synchronously, so the *orchestration*
//! lives apart (see `super::search` for the TUI side) — but the actual search
//! execution, result conversion and registration are shared here: both paths
//! call [`search_ncm`].

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use ncm_api::SongInfo;

use crate::{ipc::SearchEntry, service::ApiService, state::ContentState};

/// Registry of recently searched songs keyed by song id, shared with `App` so
/// `boxpigma msg play <id>` can enqueue and play a result that is not part of the
/// active playback queue.
pub type SearchResults = Arc<Mutex<HashMap<u64, Arc<SongInfo>>>>;

/// Searches NetEase Cloud Music on behalf of the IPC server, registering results
/// so a returned id stays resolvable in this instance.
pub struct SearchEngine {
    service: ApiService,
    search_results: SearchResults,
    limit: usize,
}

impl SearchEngine {
    pub fn new(service: ApiService, search_results: SearchResults, limit: usize) -> Self {
        Self {
            service,
            search_results,
            limit,
        }
    }

    /// Run a search against NCM, sharing the same helper as the TUI search bar.
    pub async fn search(&self, keyword: &str) -> Vec<SearchEntry> {
        let mut entries = Vec::new();

        match search_ncm(&self.service, &self.search_results, keyword, self.limit).await {
            ContentState::Songs(songs) => {
                for song in &songs {
                    entries.push(SearchEntry::from_song(song, "netease"));
                }
            }
            ContentState::Error(e) => {
                log::warn!("NCM search failed: {e}");
            }
            _ => {}
        }

        entries
    }
}

/// Search NetEase Cloud Music for `keyword` and register the hits by id so
/// `boxpigma msg play <id>` can enqueue them later. Returns the API `ContentState`
/// unchanged (the TUI surfaces the error string verbatim).
pub async fn search_ncm(
    service: &ApiService,
    search_results: &SearchResults,
    keyword: &str,
    limit: usize,
) -> ContentState {
    match service.search_songs(keyword, limit as u16).await {
        ContentState::Songs(songs) => {
            register_search_results(search_results, &songs);
            ContentState::Songs(songs)
        }
        other => other,
    }
}

/// Register a batch of search hits by id so `boxpigma msg play <id>` can later
/// enqueue and play them even though they are not part of the active queue.
pub fn register_search_results(search_results: &SearchResults, songs: &[Arc<SongInfo>]) {
    if let Ok(mut map) = search_results.lock() {
        for song in songs {
            map.insert(song.id, Arc::clone(song));
        }
    }
}
