use std::sync::Arc;

use crate::text_input::TextInput;

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub active: bool,
    pub input: TextInput,
    pub filter_queue_only: bool,
    pub unfiltered_songs: Option<Vec<Arc<ncm_api::SongInfo>>>,
}
