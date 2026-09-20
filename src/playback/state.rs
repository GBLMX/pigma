use std::{sync::Arc, time::Duration};

use ncm_api::SongInfo;

use super::{cover::CoverState, lyrics::LyricLine, mode::PlayMode, pitch::Note};

#[derive(Debug, Clone)]
pub struct PlaybackState {
    /// Playback position as a fraction of the track, for the progress bar.
    pub progress: f64,
    /// The same position in seconds. Anything that has to line up with the recording — the
    /// lyrics, the karaoke sweep — needs this and not the fraction: turning the fraction back
    /// into a time means multiplying by a duration, and if that duration is not the one the
    /// fraction was divided by (the decoder's total vs the metadata's), the whole timeline is
    /// scaled by a constant — lyrics that run at a steady but wrong rate.
    pub position_secs: f64,
    pub volume: f64,
    pub paused: bool,
    pub playing: bool,
    pub seeking: bool,
    pub current_song: Option<Arc<SongInfo>>,
    pub error: Option<String>,
    pub lyrics: Option<Vec<LyricLine>>,
    pub translated_lyrics: Option<Vec<LyricLine>>,
    pub mode: PlayMode,
    pub cached: bool,
    pub liked: bool,
    pub cover: CoverState,
    /// Bar levels of the playing audio's frequency spectrum, refreshed by the engine.
    pub visualizer: Vec<f32>,
    /// Dominant pitch of the playing audio, if one could be detected.
    pub pitch: Option<Note>,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            progress: 0.0,
            position_secs: 0.0,
            volume: 0.65,
            paused: false,
            playing: false,
            seeking: false,
            current_song: None,
            error: None,
            lyrics: None,
            translated_lyrics: None,
            mode: PlayMode::Sequential,
            cached: false,
            liked: false,
            cover: CoverState::default(),
            visualizer: Vec::new(),
            pitch: None,
        }
    }
}

impl PlaybackState {
    pub(super) fn on_started(&mut self) {
        self.error = None;
        self.paused = false;
        self.playing = true;
        self.lyrics = None;
        self.translated_lyrics = None;
    }

    pub(super) fn on_progress(&mut self, position: Duration, total: Option<Duration>) {
        self.seeking = false;
        let total_secs = match total {
            Some(t) => t.as_secs_f64(),
            None => self
                .current_song
                .as_ref()
                .map(|s| s.duration as f64 / 1000.0)
                .unwrap_or(0.0),
        };
        self.position_secs = position.as_secs_f64();
        if total_secs > 0.0 {
            self.progress = (self.position_secs / total_secs).clamp(0.0, 1.0);
        }
    }

    /// Resets progress. Returns `true` if the caller should advance to the next song.
    pub(super) fn on_finished(&mut self) -> bool {
        self.progress = 0.0;
        self.position_secs = 0.0;
        // When the track ends but nothing advances it (e.g. an empty queue winding down),
        // exit the seeking poll loop as a safety net.
        self.seeking = false;
        self.playing
    }

    pub(super) fn clear_after_stopped(&mut self) {
        self.current_song = None;
        self.error = None;
        self.paused = false;
    }

    pub(super) fn on_error(&mut self, err: String) {
        log::error!("Playback error: {}", err);
        // buffer underrun/overrun is transient — rodio recovers automatically
        if err.contains("buffer underrun") || err.contains("overrun") {
            return;
        }
        self.error = Some(err);
    }

    pub(super) fn on_lyrics_loaded(
        &mut self,
        song_id: u64,
        lyrics: Vec<LyricLine>,
        translated_lyrics: Vec<LyricLine>,
    ) {
        if let Some(song) = &self.current_song
            && song.id == song_id
        {
            self.lyrics = Some(lyrics);
            self.translated_lyrics = Some(translated_lyrics);
        }
    }
}

#[cfg(test)]
mod progress_tests {
    use super::*;

    /// The lyrics and the karaoke sweep have to line up with the recording, so they need the
    /// position itself. `progress` is a fraction of whichever total the decoder reported;
    /// turning it back into a time with the metadata's duration rescales the whole track, which
    /// is what made the lyrics run at a steady but wrong rate.
    #[test]
    fn on_progress_keeps_the_position_the_fraction_could_not_recover() {
        let mut state = PlaybackState::default();
        // The decoder reports 210s, the NetEase metadata says 200s.
        state.on_progress(
            Duration::from_secs_f64(60.0),
            Some(Duration::from_secs_f64(210.0)),
        );

        assert!(
            (state.position_secs - 60.0).abs() < 1e-9,
            "the position itself is what callers need, got {}",
            state.position_secs
        );

        // What the old path produced: 60/210 × 200 = 57.1s, three seconds behind the voice
        // here and drifting by ~1.4s per minute everywhere.
        let reconstructed = state.progress * 200.0;
        assert!(
            reconstructed < 58.0,
            "this test is pointless if the two totals agree: {reconstructed}"
        );
    }

    /// With no decoder total the fraction falls back to the metadata, and both agree.
    #[test]
    fn progress_falls_back_to_the_metadata_duration() {
        let mut state = PlaybackState::default();
        state.on_progress(Duration::from_secs_f64(50.0), None);
        assert_eq!(state.position_secs, 50.0);
        assert_eq!(
            state.progress, 0.0,
            "no song and no total: nothing to divide by"
        );

        state.on_progress(
            Duration::from_secs_f64(50.0),
            Some(Duration::from_secs_f64(100.0)),
        );
        assert!((state.progress - 0.5).abs() < 1e-9);
        assert_eq!(state.position_secs, 50.0);
    }
}
