//! Shared application state: the active `Page`, the navigation/search/login/help
//! sub-state, and `PaginationInfo` for lazy-loaded content.

pub mod avatar;
pub mod command;
pub mod content;
pub mod login;
pub mod lyrics;
pub mod mv;
pub mod navigation;
pub mod notices;
pub mod page;
pub mod popup;
pub mod prompt;
pub mod queue_page;
pub mod search;
pub mod splash;
pub mod tasks;

use std::time::Instant;

use crate::ui::playerbar::ControlButton;

pub use command::*;
pub use content::*;
pub use login::*;
pub use navigation::*;
pub use page::*;
pub use prompt::*;
pub use queue_page::*;
pub use search::*;
pub use splash::*;

// --- Private Internal Imports ---
use crate::{config::BorderConfig, event::EventHandler};
use lyrics::LyricsState;
use notices::Notices;
use popup::PopupState;
use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};
use tasks::Tasks;

/// Pagination state for a lazily-loaded content view (e.g. a playlist or
/// search results page). Drives "load more" and the loading indicator.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaginationInfo {
    /// API/endpoint key this pagination belongs to.
    pub api: String,
    /// Current offset of loaded items.
    pub offset: u32,
    /// Page size requested.
    pub limit: u32,
    /// Whether more items are available from the API.
    pub has_more: bool,
    /// Total item count reported by the API (0 when unknown).
    pub total: u64,
    /// Whether a load is currently in flight.
    pub loading: bool,
}

impl PaginationInfo {
    /// Offset of the next page after the currently loaded page.
    pub fn next_offset(&self) -> u32 {
        self.offset.saturating_add(self.limit)
    }
}
impl Default for PaginationInfo {
    fn default() -> Self {
        Self {
            api: String::new(),
            offset: 0,
            limit: 50,
            has_more: false,
            total: 0,
            loading: false,
        }
    }
}

/// A pane edge being dragged by the mouse.
#[derive(Debug, Clone, Copy)]
pub struct PaneDrag {
    pub divider: crate::layout::Divider,
    /// The pane's size when the drag started.
    pub start_size: u16,
    pub col: u16,
    pub row: u16,
}

pub struct State {
    pub running: bool,
    pub events: EventHandler,
    pub border: BorderConfig,
    pub splash: SplashState,
    pub login: LoginState,
    pub navigation: NavigationState,
    pub command_panel: CommandPanel,
    pub help: PopupState,
    pub offline: bool,
    pub tick: u64,
    pub last_tick: Instant,
    /// Where the song is in its lyrics: the current line, and the flow's colour. The page draws
    /// from it and hands it back at the end of the frame.
    pub lyrics: LyricsState,
    /// Which row of the settings page the cursor is on.
    pub settings: crate::ui::settings::SettingsState,
    /// The keys of a key sequence that is still being typed (`[keys] z z`): held between presses,
    /// and cleared by `Esc` or by the key map once the sequence is done with.
    pub pending_keys: crate::config::keymap::Chord,
    /// The frame's draggable pane edges, rebuilt by the draw pass (`ui::draw`) and consumed by
    /// mouse input — the same contract as `nav_hits`: an edge belongs to the frame that drew it.
    pub pane_dividers: crate::layout::Dividers,
    /// The pane edge being dragged, if any.
    pub pane_drag: Option<PaneDrag>,
    /// The edge that was clicked last, for the double click that collapses its pane.
    pub last_pane_click: Option<(crate::layout::Divider, Instant)>,
    /// The area the shell layout was given last frame: the room pane sizes are clamped to.
    pub shell_area: Rect,
    /// What the app has said, newest last: the toast shows the newest, `:messages` lists them all.
    pub notices: Notices,
    pub messages: PopupState,
    /// What the app is doing: the loads behind the navigation, in flight and finished.
    pub tasks: Tasks,
    pub tasks_popup: PopupState,
    /// Layout rect of the player bar, cached by the draw pass (`ui::draw`) and
    /// consumed by mouse input to hit-test volume scrolling on the player bar.
    pub playerbar_area: Rect,
    /// Rect the navigation occupies, in whichever position it is set to (empty while it is
    /// not drawn at all — a narrow terminal hides the sidebar). Cached by the draw pass for
    /// the same reason as `playerbar_area`: the wheel over it moves the navigation cursor.
    pub nav_area: Rect,
    /// Inner rect of the content table, for click-to-select.
    pub content_inner: Rect,
    /// First list index shown in that table: the draw pass pre-scrolls the rows it
    /// builds, so the widget's own offset is always zero.
    pub content_offset: usize,
    /// Progress bar rect, for click-to-seek.
    pub gauge_area: Rect,
    /// The vim-style `:` command line.
    pub prompt: PromptState,
    /// Tabs and table of the queue page, for click-to-switch and click-to-play.
    pub queue_hits: QueueHits,
    /// Rows of the artist page's hot-song and album tables, for click-to-select and for the
    /// wheel: which pane a pointer is over decides which list it walks.
    pub artist_hits: crate::state::navigation::ArtistHits,
    /// Player bar rects for click targets: the volume icon, the cover, and the row that
    /// hosts the spectrum and the pitch readout.
    pub volume_area: Rect,
    pub cover_area: Rect,
    pub spectrum_row_area: Rect,
    /// Pitch readout cell, when the layout gave it one of its own.
    pub pitch_area: Rect,
    /// Transport buttons (previous / play-pause / next), for click-to-control.
    pub(crate) transport: [(ControlButton, Rect); 3],
    /// Mode icon cell, for click-to-cycle.
    pub mode_area: Rect,
    /// Cells of the like button, one per place the player bar draws a heart.
    pub like_areas: [Rect; 2],
    /// Volume to restore when the volume icon is clicked again.
    pub volume_before_mute: Option<f64>,
}

#[cfg(test)]
mod pagination_tests {
    use super::PaginationInfo;

    #[test]
    fn next_offset_advances_past_current_page() {
        let pagination = PaginationInfo {
            offset: 60,
            limit: 60,
            ..PaginationInfo::default()
        };

        assert_eq!(pagination.next_offset(), 120);
    }

    #[test]
    fn next_offset_saturates() {
        let pagination = PaginationInfo {
            offset: u32::MAX - 5,
            limit: 60,
            ..PaginationInfo::default()
        };

        assert_eq!(pagination.next_offset(), u32::MAX);
    }
}
