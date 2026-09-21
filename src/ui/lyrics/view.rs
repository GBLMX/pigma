//! The page's view of one frame: the lyrics, where the song is in them, and the colours.

use ratatui::{
    layout::Alignment,
    style::{Color, Style},
    text::{Line, Span},
};

use crate::{
    config::{Theme, symbols},
    playback::LyricLine,
    state::lyrics::LyricsState,
    utils::GradientPreset,
};

pub(super) struct View<'a> {
    pub(super) lyrics: &'a [LyricLine],
    pub(super) translated: Option<&'a [LyricLine]>,
    pub(super) cur: usize,
    pub(super) cur_ms: f64,
    pub(super) colors: &'a Theme,
    pub(super) gradient: GradientPreset,
    /// Where the song is in these lyrics: the current line's fill, and the flow's phase.
    pub(super) state: &'a LyricsState,
    /// What `ktv` paints the sung part with.
    pub(super) ktv_color: Color,
    /// The song's length in milliseconds, when the player knows it. It is what says where the
    /// last line ends — nothing else in the file does.
    pub(super) total_ms: Option<f64>,
}

// The page takes its player, its style, where the song is, the pane sizes and the frame: they
// are what a page needs, and a struct that only ever holds them would be this list with a name.

impl<'a> View<'a> {
    /// A lyric line's text, with a dot standing in for an instrumental gap.
    pub(super) fn text(&self, i: usize) -> &'a str {
        let text = self.lyrics[i].text.as_str();
        if text.is_empty() { "·" } else { text }
    }

    /// How much of the line at `i` has been sung, in characters — where the fill has got to.
    pub(super) fn sung_chars(&self, i: usize) -> usize {
        self.state.fill(self.lyrics, i, self.cur_ms, self.total_ms)
    }

    /// The translation of line `i` drawn as its own line, marked so it reads as the translation
    /// of the line above it rather than as another lyric.
    ///
    /// Italic alone does not tell the two apart: most terminals cannot slant CJK glyphs, so a
    /// Chinese translation came out looking exactly like the English line above it.
    pub(super) fn translation_line(&self, i: usize, style: Style) -> Option<Line<'a>> {
        let text = self.translation(i)?;
        let mut line = Line::default();
        line.push_span(Span::styled(symbols().translation.as_str(), style));
        line.push_span(Span::styled(" ", style));
        line.push_span(Span::styled(text, style));

        Some(line.alignment(Alignment::Center))
    }

    /// The translation of a line, when there is one to show.
    pub(super) fn translation(&self, i: usize) -> Option<&'a str> {
        self.translated?
            .get(i)
            .map(|l| l.text.as_str())
            .filter(|t| !t.is_empty())
    }

    /// The first line of a centred window of `height` lines.
    pub(super) fn window_start(&self, height: usize, lines_per_lyric: usize) -> usize {
        self.cur.saturating_sub((height / lines_per_lyric) / 2)
    }
}
