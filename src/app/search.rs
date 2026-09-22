use super::{App, search_core::search_ncm, send_event};
use crate::{event::NavigationEvent, state::ContentState, text_input::TextInput};

impl App {
    pub(super) fn handle_search_song(&mut self, keyword: String) {
        self.submit_ncm_search(keyword);
    }

    /// TUI-only orchestration for an NCM search: mark the loading state, spawn
    /// the search (delegating to [`search_core::search_ncm`]) and hand the
    /// resulting `ContentState` to the navigation via an event.
    fn submit_ncm_search(&mut self, keyword: String) {
        self.state.navigation.set_content(ContentState::Loading);
        self.state.navigation.content_is_search = true;
        self.state.navigation.nav.subtitle = Some(format!("搜索: {keyword}"));
        self.state.navigation.content_selected = 0;
        let service = self.service.clone();
        let sender = self.state.events.sender();
        let limit = self.config.search_limit as usize;
        let search_results = self.search_results.clone();
        tokio::spawn(async move {
            let state = search_ncm(&service, &search_results, &keyword, limit).await;
            send_event(&sender, NavigationEvent::ContentLoaded(state).into());
        });
    }

    pub(super) fn handle_search_activate(&mut self) {
        let nav = &mut self.state.navigation;
        nav.search.active = true;
        nav.search.input = TextInput::new();
        nav.search.filter_queue_only = false;
        nav.search.unfiltered_songs = None;

        nav.push_breadcrumb();

        nav.nav.subtitle = None;
        nav.content_selected = 0;

        nav.nav.restore_focus_by_api("search");

        nav.set_content(ContentState::Empty);

        let api = nav.nav.selected_api();
        if let Some(api) = api {
            let sender = self.state.events.sender();
            send_event(&sender, NavigationEvent::NavSelect(api.to_string()).into());
        }
    }

    pub(super) fn handle_search_deactivate(&mut self) {
        let nav = &mut self.state.navigation;
        if nav.search.filter_queue_only {
            nav.search.filter_queue_only = false;
            if let Some(songs) = nav.search.unfiltered_songs.take() {
                self.playback.set_queue_songs(songs);
            }
        } else {
            nav.pop_breadcrumb();
        }
        nav.search.active = false;
        nav.search.input = TextInput::new();
        nav.nav.subtitle = None;
    }

    pub(super) fn handle_content_restore(&mut self) {
        let nav = &mut self.state.navigation;
        // An album opened from the artist page starts with an empty stack, so a failed pop is
        // not the end of the walk: the page that opened the content is the step back.
        if !nav.pop_breadcrumb()
            && let Some(page) = nav.return_page.take()
        {
            nav.nav.subtitle = None;
            nav.page = page;
        }
    }
}
