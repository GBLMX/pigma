//! Where the song is in its lyrics.
//!
//! The page used to ask this of a thread-local cache and of the frame counter: the current line
//! belonged to whichever thread drew last, and the flow's colour moved at whatever speed the
//! frame rate happened to be. Both live here instead — the position is a hint the `App` carries
//! between frames, and the phase advances by the clock, one pass per line.

use std::time::Instant;

use crate::playback::LyricLine;

/// How much of a pass the flow's colour may move in one frame.
///
/// A frame longer than a quarter pass — a stall, a suspended process, a debugger — moves it by
/// the cap rather than by the elapsed time, so the palette cannot slide round while nobody is
/// looking at it.
const MAX_FRAME_PASS: f32 = 0.25;

/// The line the song is on, and where the flow's colour has got to.
///
/// Held by the `App` and handed to the lyrics page as `&mut`, the way ratatui's stateful widgets
/// are: the page draws, the state remembers.
#[derive(Debug, Default)]
pub struct LyricsState {
    /// The line the previous frame was on. A hint, not a fact: it is what keeps [`current_line`]
    /// a step from where the song was rather than a scan from the top of the file.
    last_line: Option<usize>,
    /// How far the flow has travelled, in passes of the palette (0..1). Kept across lines: the
    /// speed changes with the line, and resetting the phase at every line change would make the
    /// colour jump back to the start each time one ends.
    flow_phase: f32,
    /// When the phase was last advanced.
    flow_last: Option<Instant>,
}

impl LyricsState {
    /// The line the song is on at `cur_ms`, remembered for the next frame.
    pub fn current_line(&mut self, lyrics: &[LyricLine], cur_ms: f64) -> usize {
        let line = hint_line(lyrics, self.last_line, cur_ms);
        self.last_line = Some(line);

        line
    }

    /// How far the flow has travelled, in passes of the palette (0..1).
    pub fn flow_phase(&self) -> f32 {
        self.flow_phase
    }

    /// How many characters of the line at `i` the voice has passed — where the fill has got to.
    pub fn fill(
        &self,
        lyrics: &[LyricLine],
        i: usize,
        cur_ms: f64,
        total_ms: Option<f64>,
    ) -> usize {
        fill(lyrics, i, cur_ms, total_ms)
    }

    /// Advance the flow's colour to `now` and hand back where it stands, in passes.
    ///
    /// One pass takes one line: the colour moves as fast as the line is sung, so a quick line
    /// moves it quickly. Nothing moves while the song is paused — the colour is where the voice
    /// left it — and a line whose length is unknown does not move it either, since there is no
    /// speed to derive.
    pub fn flow(&mut self, playing: bool, line_ms: f64, now: Instant) -> f32 {
        let previous = self.flow_last.replace(now);
        if let Some(previous) = previous
            && playing
            && line_ms > 0.0
        {
            let elapsed = now.saturating_duration_since(previous).as_secs_f64() * 1000.0;
            let passes = (elapsed / line_ms).clamp(0.0, f64::from(MAX_FRAME_PASS)) as f32;
            self.flow_phase = (self.flow_phase + passes).rem_euclid(1.0);
        }

        self.flow_phase
    }
}

/// The line at `cur_ms`, scanned from `hint`.
///
/// Forward from the hint, which is where a song spends its time; backwards only when the song has
/// gone back — a seek, or a new song shorter than the old one — so a seek is still one lookup
/// rather than a scan. With no hint, or a hint that no longer fits the file, the line is found by
/// search: an intro before the first line belongs to the first line rather than to no line at all.
pub fn hint_line(lyrics: &[LyricLine], hint: Option<usize>, cur_ms: f64) -> usize {
    if lyrics.is_empty() {
        return 0;
    }

    let mut cur = hint.filter(|line| *line < lyrics.len()).unwrap_or(0);
    if lyrics[cur].time.as_millis() as f64 > cur_ms {
        return lyrics
            .iter()
            .rposition(|line| line.time.as_millis() as f64 <= cur_ms)
            .unwrap_or(0);
    }

    while cur + 1 < lyrics.len() && lyrics[cur + 1].time.as_millis() as f64 <= cur_ms {
        cur += 1;
    }

    cur
}

/// How many characters of the line at `i` have been sung at `cur_ms`.
///
/// The line's own duration is what the fraction is measured against — see [`line_duration`] —
/// so a line with no known end does not fill up the moment it starts.
pub fn fill(lyrics: &[LyricLine], i: usize, cur_ms: f64, total_ms: Option<f64>) -> usize {
    let Some(line) = lyrics.get(i) else {
        return 0;
    };

    let start = line.time.as_millis() as f64;
    let progress = ((cur_ms - start) / line_duration(lyrics, i, total_ms)).clamp(0.0, 1.0);

    (line.text.chars().count() as f64 * progress).floor() as usize
}

/// How long the line at `i` lasts, in milliseconds.
///
/// The next line's start is the honest answer, and the only duration the file itself offers. The
/// animations that follow a line need a positive one all the same: the last line gets the rest of
/// the song, or [`LAST_LINE_FALLBACK_MS`] when even that is unknown.
///
/// This is what replaced "the next line's start, or zero for the last line": `unwrap_or_default()`
/// made the fill jump straight to complete on the last line, and the window handed
/// `lyrics[i + 1]` straight to `Duration` handling, which panicked at the end of every song.
pub fn line_duration(lyrics: &[LyricLine], i: usize, total_ms: Option<f64>) -> f64 {
    /// What the last line is given when the song's own length is unknown.
    const LAST_LINE_FALLBACK_MS: f64 = 4_000.0;

    let Some(line) = lyrics.get(i) else {
        return LAST_LINE_FALLBACK_MS;
    };
    let start = line.time.as_millis() as f64;

    if lyrics.get(i + 1).is_some() {
        return (lyrics[i + 1].time.as_millis() as f64 - start).max(1.0);
    }
    match total_ms {
        Some(total) if total > start => (total - start).max(1.0),
        _ => LAST_LINE_FALLBACK_MS,
    }
}

/// The song's own length in milliseconds, when the API reported one.
///
/// A zero is a missing length, not a song of no length: it is what the field holds for a song
/// whose length never arrived, and it would otherwise put the last line at the very start of the
/// timeline.
pub fn song_duration_ms(duration_ms: u64) -> Option<f64> {
    (duration_ms > 0).then_some(duration_ms as f64)
}

#[cfg(test)]
mod fixtures {
    use std::time::Duration;

    use crate::playback::LyricLine;

    /// Six lines, five seconds apart: one per five seconds of the song, which makes every
    /// expectation in these tests a division by five.
    pub fn lyrics() -> Vec<LyricLine> {
        (0..6)
            .map(|i| LyricLine {
                time: Duration::from_millis(i * 5_000),
                text: format!("line{i}"),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{fixtures::lyrics, *};

    /// The line being sung is the last one that has started: the instant a line begins belongs to
    /// that line, and an intro before the first one belongs to the first one rather than to none.
    #[test]
    fn the_current_line_is_the_one_being_sung_at_that_instant() {
        let lines = lyrics();

        assert_eq!(hint_line(&lines, None, 0.0), 0);
        assert_eq!(hint_line(&lines, None, 4_999.0), 0);
        assert_eq!(hint_line(&lines, None, 5_000.0), 1);
        assert_eq!(hint_line(&lines, None, 27_000.0), 5);
        assert_eq!(hint_line(&lines, None, 900_000.0), 5);
    }

    /// An empty file is not indexed into: a song whose lyrics never arrived has no line to draw.
    #[test]
    fn an_empty_file_has_no_current_line() {
        assert_eq!(hint_line(&[], None, 12_000.0), 0);
        assert_eq!(fill(&[], 0, 12_000.0, None), 0);
    }

    /// The hint makes the common case a step rather than a scan: a stale one is corrected, in
    /// both directions, without ever returning a line the song is not on.
    #[test]
    fn a_stale_hint_is_corrected_in_both_directions() {
        let lines = lyrics();

        // The song moved on while the hint stayed behind.
        assert_eq!(hint_line(&lines, Some(1), 21_500.0), 4);
        // The song went back: a seek, or a new song shorter than the one before it.
        assert_eq!(hint_line(&lines, Some(5), 1_000.0), 0);
        // A hint from a longer song cannot index this one.
        assert_eq!(hint_line(&lines, Some(99), 11_000.0), 2);
    }

    /// The hint is the state's own memory: it follows the song forward frame by frame, and a
    /// seek backwards is still answered correctly — with the same answer a scan would give.
    #[test]
    fn the_hint_follows_the_song_across_frames() {
        let lines = lyrics();
        let mut state = LyricsState::default();

        for (ms, expected) in [
            (0.0, 0),
            (6_000.0, 1),
            (11_000.0, 2),
            (26_000.0, 5),
            (1_000.0, 0),
        ] {
            assert_eq!(state.current_line(&lines, ms), expected, "at {ms} ms");
        }
    }

    /// A line lasts until the next one starts, and the last line lasts to the end of the song —
    /// with a positive length even when nothing says when the song ends.
    #[test]
    fn a_line_lasts_until_the_next_one_starts() {
        let lines = lyrics();

        assert_eq!(line_duration(&lines, 0, None), 5_000.0);
        assert_eq!(line_duration(&lines, 4, None), 5_000.0);
        // The last line: the rest of the song when its length is known…
        assert_eq!(line_duration(&lines, 5, Some(40_000.0)), 15_000.0);
        // …and a positive fallback when it is not, or when it makes no sense.
        assert_eq!(line_duration(&lines, 5, None), 4_000.0);
        assert_eq!(line_duration(&lines, 5, Some(1_000.0)), 4_000.0);
        assert_eq!(song_duration_ms(0), None);
        assert_eq!(song_duration_ms(258_000), Some(258_000.0));
    }

    /// The fill grows with the voice, a character at a time, and never runs past either end.
    #[test]
    fn the_sung_part_grows_with_the_line() {
        let lines = lyrics();

        assert_eq!(fill(&lines, 1, 5_000.0, None), 0);
        // Half of a five-character line is two characters and a half, and a fill can only draw
        // whole ones.
        assert_eq!(fill(&lines, 1, 7_500.0, None), 2);
        assert_eq!(fill(&lines, 1, 8_000.0, None), 3);
        assert_eq!(fill(&lines, 1, 10_000.0, None), 5);
        // Past the line's end the fill is complete rather than over it, and before it began it is
        // empty rather than negative.
        assert_eq!(fill(&lines, 1, 40_000.0, None), 5);
        assert_eq!(fill(&lines, 1, 0.0, None), 0);
    }

    /// One pass takes one line: the colour moves as fast as the line is sung. The phase is
    /// shared between lines, so a line change does not jump it back to the start.
    #[test]
    fn a_pass_takes_as_long_as_the_line() {
        // Frames of half a second, well inside the per-frame cap, so what these tests measure is
        // the speed the line sets and not the cap.
        const FRAME: Duration = Duration::from_millis(500);
        let start = Instant::now();

        // Half of a five-second line is half a pass.
        let mut state = LyricsState::default();
        state.flow(true, 5_000.0, start);
        let mut now = start;
        for _ in 0..5 {
            now += FRAME;
            state.flow(true, 5_000.0, now);
        }
        assert_eq!(state.flow(true, 5_000.0, now), 0.5);

        // The same half second on a line half as long moves it twice as far: a quicker line
        // moves the colour quicker.
        let mut quick = LyricsState::default();
        quick.flow(true, 2_500.0, start);
        assert_eq!(quick.flow(true, 2_500.0, start + FRAME), 0.2);
    }

    /// The phase is carried across lines rather than restarted: a line change is not a reason for
    /// the colour to jump back to the start of the palette.
    #[test]
    fn a_line_change_keeps_the_phase() {
        let start = Instant::now();
        let mut state = LyricsState::default();

        state.flow(true, 5_000.0, start);
        state.flow(true, 5_000.0, start + Duration::from_millis(1_000));
        // The next line is quicker; the phase it starts from is where the last one left it.
        assert_eq!(
            state.flow(true, 2_500.0, start + Duration::from_millis(1_000)),
            0.2
        );
        assert_eq!(
            state.flow(true, 2_500.0, start + Duration::from_millis(1_500)),
            0.4
        );
    }

    /// A stall moves the colour by the cap, not by the time that passed: a suspended process must
    /// not come back with the palette a whole turn further round.
    #[test]
    fn a_long_frame_is_capped() {
        let mut state = LyricsState::default();
        let start = Instant::now();

        state.flow(true, 2_000.0, start);
        assert_eq!(
            state.flow(true, 2_000.0, start + Duration::from_secs(30)),
            MAX_FRAME_PASS
        );
    }

    /// The colour only moves while the song does.
    #[test]
    fn the_phase_only_moves_while_playing() {
        let mut state = LyricsState::default();
        let start = Instant::now();

        state.flow(true, 5_000.0, start);
        let paused = state.flow(false, 5_000.0, start + Duration::from_millis(1_000));
        assert_eq!(
            paused, 0.0,
            "paused, the colour stays where the voice left it"
        );

        // Time spent paused is not a pass either: the next playing frame moves on from where the
        // voice stopped, by one frame's worth.
        assert_eq!(
            state.flow(true, 5_000.0, start + Duration::from_millis(2_000)),
            0.2
        );
        assert_eq!(
            state.flow(true, 5_000.0, start + Duration::from_millis(2_500)),
            0.3
        );
    }

    /// A line whose length is unknown has no speed to move at — the colour holds rather than
    /// jumping on the next frame's guess.
    #[test]
    fn a_line_of_unknown_length_holds_the_phase() {
        let mut state = LyricsState::default();
        let start = Instant::now();

        state.flow(true, 0.0, start);
        assert_eq!(
            state.flow(true, 0.0, start + Duration::from_millis(1_000)),
            0.0
        );
    }
}
