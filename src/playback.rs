//! Audio playback subsystem: re-exports the `PlaybackEngine` plus the queue/scan/
//! lyrics/cover state and play-mode types used across the app.

mod chain;
mod controller;
mod cover;
mod dsp;
mod engine;
mod exclusive;
mod heartbeat;
mod lyrics;
mod mode;
mod pitch;
mod player;
mod queue;
mod scan;
mod source;
mod spectrum;
mod state;
mod storage;
mod stream_client;

pub use cover::CoverState;
pub use engine::{NCM_SEARCH_QUEUE_KEY, PlaybackEngine, THIRD_PARTY_QUEUE_KEY};
pub use lyrics::{LyricLine, parse_lyric_lines};
pub use mode::{PlayMode, mode_icon};
pub use pitch::{Note, PitchTracker, note_from_frequency};
pub use scan::scan_local_music;
pub use spectrum::{BANDS, Spectrum, SpectrumBuffer, SpectrumTap};
pub use state::PlaybackState;
