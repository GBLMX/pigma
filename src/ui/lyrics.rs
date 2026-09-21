
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Padding, Paragraph, Wrap},
};
use ratatui_image::{Resize, StatefulImage};

use super::{BlockStyle, block::CornerBlock};
use crate::{
    config::{
        LyricStyle, Pane, PanesConfig, Theme,
        lyrics::LyricsConfig,
        symbols,
    },
    layout::{Axis, Divider, Dividers, clamp},
    playback::{LyricLine, PlaybackState},
    state::{
        lyrics::LyricsState,
        mv::{self, MvPanel},
    },
    utils::{GradientPreset, format::clip_long_text, format_duration},
};

// Where the song is in its lyrics — the current line, the fill, and the flow's phase — lives in
// `state::lyrics`: it used to live here, in a thread-local cache that belonged to whichever
// thread drew last, and in the frame counter.

/// Everything the four presentations share: the song's lyrics, where the recording is, and
/// the colours and gradient to draw them with.
struct View<'a> {
    lyrics: &'a [LyricLine],
    translated: Option<&'a [LyricLine]>,
    cur: usize,
    cur_ms: f64,
    colors: &'a Theme,
    gradient: GradientPreset,
    /// Where the song is in these lyrics: the current line's fill, and the flow's phase.
    state: &'a LyricsState,
    /// What `ktv` paints the sung part with.
    ktv_color: Color,
    /// The song's length in milliseconds, when the player knows it. It is what says where the
    /// last line ends — nothing else in the file does.
    total_ms: Option<f64>,
}

// The page takes its player, its style, where the song is, the pane sizes and the frame: they
// are what a page needs, and a struct that only ever holds them would be this list with a name.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw(
    f: &mut Frame,
    player: &PlaybackState,
    bs: &BlockStyle<'_>,
    options: &LyricsConfig<'_>,
    state: &mut LyricsState,
    panes: &PanesConfig,
    dividers: &mut Dividers,
    area: Rect,
) {
    let gradient = options.gradient;
    let style = options.style;
    let colors = bs.colors;
    let block = CornerBlock::from_color(bs, bs.base).title(options.title, bs.colors);
    let inner = block.inner(area);
    f.render_widget(block.block_padding(Padding::vertical(1)), area);

    if player.current_song.is_none() {
        return;
    }

    // The MV's poster, when the song has one and it has landed, takes a column off the right
    // edge; `panel_split` leaves the lyrics the whole page otherwise, down to the byte. It
    // belongs to the song rather than to its lyrics, so it is drawn whether or not they have
    // arrived — and it is what is left of the page while they are still on their way.
    let mut slot = mv::panel();
    let (inner, panel_area) = panel_split(inner, panes, slot.is_some());
    if let Some(panel_area) = panel_area {
        if let Some(panel) = slot.as_mut() {
            draw_panel(f, panel_area, panel, colors);
        }
        // The column's left edge is draggable — the same edge the sidebar has on the other side,
        // and the same `[panes]` machinery. Dragging it left makes the column wider, so the sign
        // is negative; double-clicking it collapses the pane, like the sidebar's.
        dividers.push(Divider::new(
            Pane::Mv,
            Axis::Columns,
            -1,
            Rect::new(panel_area.x.saturating_sub(1), panel_area.y, 1, panel_area.height),
        ));
    }

    let Some(lyrics) = &player.lyrics else {
        return;
    };

    if lyrics.is_empty() {
        let msg = Line::from("纯音乐，请欣赏")
            .style(Style::default().fg(colors.muted))
            .alignment(Alignment::Center);
        f.render_widget(Paragraph::new(msg), inner);
        return;
    }

    // The position, not `progress × duration`: the fraction was divided by whichever total
    // the decoder reported, so multiplying it back by the metadata's duration scales the whole
    // lyric timeline by a constant — lines that run at a steady but wrong rate.
    let cur_ms = player.position_secs * 1000.0;
    let cur = state.current_line(lyrics, cur_ms);

    let view = View {
        lyrics,
        translated: player
            .translated_lyrics
            .as_deref()
            .filter(|_| options.show_translation)
            .filter(|t| !t.is_empty()),
        cur,
        cur_ms,
        colors,
        gradient,
        state,
        ktv_color: options.ktv_color,
        total_ms: player
            .current_song
            .as_ref()
            .map(|song| song.duration as f64)
            .filter(|ms| *ms > 0.0),
    };

    match style {
        LyricStyle::Window => draw_window(f, &view, inner, Fill::Gradient),
        LyricStyle::OneLine => draw_one_line(f, &view, inner),
        LyricStyle::Ktv => draw_window(f, &view, inner, Fill::Ktv),
        LyricStyle::Flow => draw_flow(f, &view, inner),
        LyricStyle::Plain => draw_plain(f, &view, inner),
    }
}

/// The tallest poster the panel draws, in rows. A poster is a square picture, so at the roughly
/// 1:2 character cell this is 24 cells across — a quarter of the width on a 96-cell terminal,
/// which is a poster beside the lyrics rather than the page itself.
const MAX_POSTER_ROWS: u16 = 12;

/// The widest line the facts can make, in cells: `03:45 · 1993-09-09`, a length and a release
/// date. The poster is never narrower than this, so those facts are always written whole and the
/// blurb is the only thing a small page cuts.
const FACTS_WIDTH: u16 = 18;

/// The shortest poster the panel draws, in rows — as wide as the facts need, and a square
/// picture is half as many rows tall. Anything less is a thumbnail, and the page is better off
/// with the lyrics alone.
const MIN_POSTER_ROWS: u16 = FACTS_WIDTH / 2;

/// Rows the facts take under the poster: the title, the singer, and the length with the release
/// date.
const FACTS_ROWS: u16 = 3;

/// Every row of the column that is not the poster: the three lines of facts, the blank under
/// them, and the one row of blurb a panel is worth having for at all.
const PANEL_TEXT_ROWS: u16 = FACTS_ROWS + 1 + 1;

/// The shortest page that gets a panel: the smallest poster and the rows under it.
const MIN_PANEL_HEIGHT: u16 = MIN_POSTER_ROWS + PANEL_TEXT_ROWS;

/// Cells between the lyrics and the poster column, so a long lyric line never runs into the
/// poster.
const PANEL_GAP: u16 = 2;

/// What the lyrics keep of the page: narrower than this and the poster would be the page.
const MIN_LYRICS_WIDTH: u16 = 30;

/// Divide `inner` between the lyrics and the MV's column, which is the rightmost one.
///
/// The column's width is `[panes] mv` — the size the user dragged it to, or left at 0 for the
/// width the page can spare: as tall as the page has rows for, up to [`MAX_POSTER_ROWS`], and the
/// column twice that, since a poster is a square picture. So a page with rows to give gets the
/// big poster, a shorter one a smaller poster, and a page that was never dragged keeps exactly
/// what it drew before the panel was a pane.
///
/// Returns the lyrics area and `None` when there is no panel to draw, or no room for one: the
/// lyrics then keep the whole page, which is what makes a page without a poster — and a page too
/// small for one — exactly the page this drew before the panel existed.
fn panel_split(inner: Rect, panes: &PanesConfig, has_panel: bool) -> (Rect, Option<Rect>) {
    if !has_panel || !panes.visible(Pane::Mv) || inner.height < MIN_PANEL_HEIGHT {
        return (inner, None);
    }

    // A dragged width is clamped the way the other panes are (their config stays usable on a
    // narrower terminal than the one it was written on); the automatic one is only used when the
    // page really has the room, so a page too small for a poster is still a page of lyrics.
    let width = match panes.size(Pane::Mv) {
        0 => poster_rows(inner.height) * 2,
        dragged => clamp(Pane::Mv, Axis::Columns, dragged, inner.width),
    };
    if inner.width < width + PANEL_GAP + MIN_LYRICS_WIDTH {
        return (inner, None);
    }

    let [lyrics, _, panel] = Layout::horizontal([
        Constraint::Min(MIN_LYRICS_WIDTH),
        Constraint::Length(PANEL_GAP),
        Constraint::Length(width),
    ])
    .areas(inner);
    (lyrics, Some(panel))
}

/// Rows the poster takes on a page of `height` rows: everything the text under it does not need,
/// up to the tallest poster the panel draws.
fn poster_rows(height: u16) -> u16 {
    height.saturating_sub(PANEL_TEXT_ROWS).min(MAX_POSTER_ROWS)
}

/// Draw the MV in its column: the poster, and under it what the API knows about it — the title,
/// the singer, the length with the release date, and as much of the blurb as the rows left under
/// those can hold. The blurb wraps into its own area and is cut by it, the way the artist page's
/// biography is, so it can never reach the frame or the lyrics beside it.
fn draw_panel(f: &mut Frame, area: Rect, panel: &mut MvPanel, colors: &Theme) {
    // The column is twice as wide as the poster is tall, so this is the same square
    // [`panel_split`] gave the column; the facts start directly under it.
    let [poster, text] =
        Layout::vertical([Constraint::Length(area.width / 2), Constraint::Min(0)]).areas(area);

    // No mask: a poster is a rectangle, where the disc in the player bar is a circle.
    f.render_stateful_widget(
        StatefulImage::new().resize(Resize::Fit(None)),
        poster,
        &mut panel.poster,
    );

    let info = &panel.info;
    let width = usize::from(text.width);
    let mut facts = format_duration(info.duration);
    if !info.publish_time.is_empty() {
        facts.push_str(" · ");
        facts.push_str(&info.publish_time);
    }

    let [facts_area, _, blurb_area] = Layout::vertical([
        Constraint::Length(FACTS_ROWS),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(text);

    let lines = vec![
        Line::from(Span::styled(
            clip_long_text(&info.name, width),
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            clip_long_text(&info.artist_name, width),
            Style::default().fg(colors.text),
        )),
        Line::from(Span::styled(
            clip_long_text(&facts, width),
            Style::default().fg(colors.muted),
        )),
    ];
    f.render_widget(Paragraph::new(lines), facts_area);

    if blurb_area.is_empty() || info.desc.trim().is_empty() {
        return;
    }
    let blurb = Paragraph::new(info.desc.trim())
        .style(Style::default().fg(colors.muted))
        .wrap(Wrap { trim: true });
    f.render_widget(blurb, blurb_area);
}

impl<'a> View<'a> {
    /// A lyric line's text, with a dot standing in for an instrumental gap.
    fn text(&self, i: usize) -> &'a str {
        let text = self.lyrics[i].text.as_str();
        if text.is_empty() { "·" } else { text }
    }

    /// How much of the line at `i` has been sung, in characters — where the fill has got to.
    fn sung_chars(&self, i: usize) -> usize {
        self.state.fill(self.lyrics, i, self.cur_ms, self.total_ms)
    }

    /// The translation of line `i` drawn as its own line, marked so it reads as the translation
    /// of the line above it rather than as another lyric.
    ///
    /// Italic alone does not tell the two apart: most terminals cannot slant CJK glyphs, so a
    /// Chinese translation came out looking exactly like the English line above it.
    fn translation_line(&self, i: usize, style: Style) -> Option<Line<'a>> {
        let text = self.translation(i)?;
        let mut line = Line::default();
        line.push_span(Span::styled(symbols().translation.as_str(), style));
        line.push_span(Span::styled(" ", style));
        line.push_span(Span::styled(text, style));

        Some(line.alignment(Alignment::Center))
    }

    /// The translation of a line, when there is one to show.
    fn translation(&self, i: usize) -> Option<&'a str> {
        self.translated?
            .get(i)
            .map(|l| l.text.as_str())
            .filter(|t| !t.is_empty())
    }

    /// The first line of a centred window of `height` lines.
    fn window_start(&self, height: usize, lines_per_lyric: usize) -> usize {
        self.cur.saturating_sub((height / lines_per_lyric) / 2)
    }
}

/// How the line being sung is painted in the scrolling window.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fill {
    /// The gradient: the sung part forward, the rest of the line reversed.
    Gradient,
    /// One flat colour over the sung part, the way a karaoke screen covers a word.
    Ktv,
}

/// The default: a scrolling window of the lines around the current one.
fn draw_window(f: &mut Frame, view: &View<'_>, inner: Rect, fill: Fill) {
    let h = inner.height as usize;
    let lines_per_lyric = if view.translated.is_some() { 2 } else { 1 };
    let start = view.window_start(h, lines_per_lyric);
    let end = (start + h / lines_per_lyric).min(view.lyrics.len());

    let mut lines: Vec<Line> = Vec::new();
    for i in start..end {
        if i == view.cur {
            lines.push(match fill {
                Fill::Gradient => karaoke_line(view, i),
                Fill::Ktv => ktv_line(view, i),
            });
        } else {
            // Theme colours, not fixed greys: a hardcoded grey cannot follow a light theme.
            let d = i.abs_diff(view.cur);
            let style = if d <= 2 {
                Style::default().fg(view.colors.muted)
            } else {
                Style::default().fg(view.colors.border)
            };
            lines.push(Line::from(view.text(i)).style(style));
        }

        let t_style = if i == view.cur {
            Style::default()
                .fg(view.colors.text)
                .add_modifier(Modifier::ITALIC)
        } else {
            let d = i.abs_diff(view.cur);
            if d <= 2 {
                Style::default().fg(view.colors.muted)
            } else {
                Style::default().fg(view.colors.border)
            }
        };
        if let Some(translation) = view.translation_line(i, t_style) {
            lines.push(translation);
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// One line at a time: the current line alone in the middle of the page, with the same
/// karaoke fill, so there is nothing else to read.
fn draw_one_line(f: &mut Frame, view: &View<'_>, inner: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    let blank = (inner.height as usize / 2).saturating_sub(1);
    for _ in 0..blank {
        lines.push(Line::default());
    }

    lines.push(karaoke_line(view, view.cur));

    if let Some(translation) = view.translation_line(
        view.cur,
        Style::default()
            .fg(view.colors.text)
            .add_modifier(Modifier::ITALIC),
    ) {
        lines.push(translation);
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// The gradient runs along the text and moves with the music, and the neighbouring lines
/// carry the same gradient dimmed towards the background, so the page flows as a whole.
fn draw_flow(f: &mut Frame, view: &View<'_>, inner: Rect) {
    let h = inner.height as usize;
    let lines_per_lyric = if view.translated.is_some() { 2 } else { 1 };
    let start = view.window_start(h, lines_per_lyric);
    let end = (start + h / lines_per_lyric).min(view.lyrics.len());

    let mut lines: Vec<Line> = Vec::new();
    for i in start..end {
        let d = i.abs_diff(view.cur);
        // The current line at full strength, its neighbours progressively faded back.
        let keep = match (i == view.cur, d) {
            (true, _) => 1.0,
            (false, 0..=2) => 0.45,
            _ => 0.25,
        };
        lines.push(flow_line(view, view.text(i), keep, false));

        if let Some(translation) = view.translation(i) {
            let t_keep = if i == view.cur { 0.7 } else { 0.25 };
            lines.push(
                flow_line(view, translation, t_keep, true)
                    .style(Style::default().add_modifier(Modifier::ITALIC)),
            );
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// A plain scrolling list: every line in the theme's text colour, nothing highlighted.
fn draw_plain(f: &mut Frame, view: &View<'_>, inner: Rect) {
    let h = inner.height as usize;
    let start = view.window_start(h, 1);
    let end = (start + h).min(view.lyrics.len());

    let lines: Vec<Line> = (start..end)
        .map(|i| Line::from(view.text(i)).style(Style::default().fg(view.colors.text)))
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

/// A line's length in milliseconds: when the next one starts, or — for the last line, which has
/// no next one — what is left of the song, when the player knows how long that is.
///
/// An `.lrc` says when a line *starts* and nothing about when it ends, so "until the next line"
/// is the only duration the file itself offers. The animations that follow a line need a positive
/// one all the same: the last line gets the rest of the song, or [`LAST_LINE_FALLBACK_MS`] when
/// even that is unknown.
///
/// This is what replaced the old "the next line's start, or zero for the last line":
/// `unwrap_or_default()` on the last line made the fill jump straight to complete, and the window
/// passed `lyrics[i + 1]` straight through, which panicked at the end of every song.
/// The karaoke-screen fill: everything up to the voice is one flat colour — blue, unless the
/// config says otherwise — and the rest of the line stays in the theme's text colour, so the
/// words are simply covered as they are sung instead of being tinted by the gradient.
fn ktv_line<'a>(view: &View<'a>, i: usize) -> Line<'a> {
    let text = view.text(i);
    let split_at = view.sung_chars(i);
    let color = view.ktv_color;

    let mut line = Line::default();
    for (j, (byte_start, ch)) in text.char_indices().enumerate() {
        let style = if j < split_at {
            // Bold as well as coloured: a KTV sweep is a change of colour *and* weight, and it
            // keeps the edge visible when the colour is close to the line's own.
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        } else if j == split_at {
            // The character the voice is on: a block of the same colour, so the edge of the
            // fill is visible even when the sung colour is the colour of the text.
            Style::default().fg(view.colors.bg).bg(color)
        } else {
            Style::default().fg(view.colors.text)
        };
        line.push_span(Span::styled(
            &text[byte_start..byte_start + ch.len_utf8()],
            style,
        ));
    }

    line.alignment(Alignment::Center)
}

/// The karaoke fill: what has been sung is painted with the gradient, the rest gets the same
/// gradient reversed, and the boundary character marks where the voice is.
fn karaoke_line<'a>(view: &View<'a>, i: usize) -> Line<'a> {
    let text = view.text(i);
    let gradient = view.gradient;

    let total = text.chars().count();
    let split_at = view.sung_chars(i);

    let mut line = Line::default();
    for (j, (byte_start, ch)) in text.char_indices().enumerate() {
        let byte_end = byte_start + ch.len_utf8();
        let ch_str = &text[byte_start..byte_end];
        let s = if j < split_at {
            let t = j as f64 / split_at.max(1) as f64;
            let [r, g, b] = gradient.color(t as f32);
            Span::styled(ch_str, Style::default().fg(Color::Rgb(r, g, b)))
        } else if j == split_at {
            Span::styled(
                ch_str,
                Style::default()
                    .fg(Color::White)
                    .bg(view.colors.accent)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            let t = (j - split_at) as f64 / (total - split_at).max(1) as f64;
            let [r, g, b] = gradient.color(1.0 - t as f32);
            Span::styled(ch_str, Style::default().fg(Color::Rgb(r, g, b)))
        };
        line.push_span(s);
    }

    line.alignment(Alignment::Center)
}

/// One line with the gradient flowing through it: a character's colour is its position along
/// the line plus a phase that advances with the frames, so the colours travel through the
/// words instead of standing still.
fn flow_line<'a>(view: &View<'a>, text: &'a str, keep: f32, marker: bool) -> Line<'a> {
    let phase = view.state.flow_phase();
    let total = text.chars().count().max(1);
    let mut line = Line::default();

    if marker {
        let [r, g, b] = view.gradient.color(phase.rem_euclid(1.0));
        let style = Style::default().fg(fade([r, g, b], view.colors.bg, keep));
        line.push_span(Span::styled(symbols().translation.as_str(), style));
        line.push_span(Span::styled(" ", style));
    }

    for (j, (byte_start, ch)) in text.char_indices().enumerate() {
        let t = (j as f32 / total as f32 + phase).rem_euclid(1.0);
        let [r, g, b] = view.gradient.color(t);
        let color = fade([r, g, b], view.colors.bg, keep);
        line.push_span(Span::styled(
            &text[byte_start..byte_start + ch.len_utf8()],
            Style::default().fg(color),
        ));
    }

    line.alignment(Alignment::Center)
}

/// Keep `keep` of `color` and mix the rest with `background`, for the lines that should sit
/// behind the one being sung.
fn fade(color: [u8; 3], background: Color, keep: f32) -> Color {
    let Color::Rgb(br, bg, bb) = background else {
        return Color::Rgb(color[0], color[1], color[2]);
    };
    let mix = |c: u8, b: u8| {
        (f32::from(c) * keep + f32::from(b) * (1.0 - keep))
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color::Rgb(mix(color[0], br), mix(color[1], bg), mix(color[2], bb))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    use super::*;
    use crate::config::Theme;

    pub(super) fn lyrics() -> Vec<LyricLine> {
        (0..12)
            .map(|i| LyricLine {
                time: std::time::Duration::from_millis(i * 5_000),
                text: format!("line{i}"),
            })
            .collect()
    }

    /// The same shape as [`lyrics`], as a translation: what a translated file hands the player.
    fn translated_lyrics() -> Vec<LyricLine> {
        (0..12)
            .map(|i| LyricLine {
                time: std::time::Duration::from_millis(i * 5_000),
                text: format!("译文{i}"),
            })
            .collect()
    }

    /// A translation is drawn under its own line — and marked, because italic alone does not
    /// separate it from the line above: most terminals cannot slant CJK glyphs, so without the
    /// marker the two rows read as two lyrics.
    #[test]
    fn a_translation_is_drawn_marked_under_its_line() {
        let rows = symbols(&render_full(
            LyricStyle::Window,
            2.5,
            0,
            KTV,
            true,
            Some(translated_lyrics()),
        ));
        let marker = crate::config::symbols().translation.clone();

        // Every line in the window is followed by its own translation, marker and all — a
        // translation without its marker reads as the next lyric, and one under the wrong line
        // misreads the song. (The rows keep a blank for the second cell of each wide character,
        // so the comparison drops spaces.)
        let flat: Vec<String> = rows.iter().map(|row| row.replace(' ', "")).collect();
        let pairs: Vec<(&String, &String)> = flat
            .iter()
            .zip(flat.iter().skip(1))
            .filter(|(original, _)| original.starts_with("line"))
            .collect();

        assert!(pairs.len() >= 3, "{rows:?}");
        for (original, translation) in pairs {
            let n = original.trim_start_matches("line");
            assert_eq!(translation, &format!("{marker}译文{n}"), "{rows:?}");
        }
    }

    /// The switch is what decides whether they are drawn; the file decides whether there is
    /// anything to draw — and the two are independent.
    #[test]
    fn the_translation_switch_hides_translations() {
        let shown = symbols(&render_full(
            LyricStyle::Window,
            2.5,
            0,
            KTV,
            true,
            Some(translated_lyrics()),
        ));
        let hidden = symbols(&render_full(
            LyricStyle::Window,
            2.5,
            0,
            KTV,
            false,
            Some(translated_lyrics()),
        ));

        assert!(shown.iter().any(|r| r.contains('译')), "{shown:?}");
        assert!(!hidden.iter().any(|r| r.contains('译')), "{hidden:?}");
    }

    /// `ktv` paints the sung part in one flat colour — that is what makes it a karaoke screen
    /// rather than a tinted line. The gradient style is what it deliberately is not.
    #[test]
    fn ktv_fills_the_sung_part_with_one_colour() {
        let ktv = colored_cells(&render(LyricStyle::Ktv, 2_500.0, 0));
        let gradient = colored_cells(&render(LyricStyle::Window, 2_500.0, 0));

        assert!(ktv.contains(&KTV), "{ktv:?}");
        assert!(
            !gradient.contains(&KTV),
            "the gradient style must not paint with the ktv colour: {gradient:?}"
        );
    }

    /// A blue that is none of the theme's own colours, so "the sung part is painted with
    /// `lyric_ktv_color`" is distinguishable from "it is the text colour".
    const KTV: Color = Color::Rgb(77, 166, 255);

    fn render(style: LyricStyle, position_secs: f64, tick: u64) -> Buffer {
        render_full(style, position_secs, tick, KTV, true, None)
    }

    /// The same, with a translation attached and the two switches the page obeys.
    /// `flow_ticks` is how far the page's flow clock has run: 0 is a fresh phase and each tick is
    /// a tenth of a pass — advanced through the state's own clock rather than set behind its back,
    /// so the phase a test draws with is one the app could really have.
    fn render_full(
        style: LyricStyle,
        position_secs: f64,
        flow_ticks: u64,
        ktv_color: Color,
        show_translation: bool,
        translated: Option<Vec<LyricLine>>,
    ) -> Buffer {
        let theme = Theme::default();
        // A line with a gradient that differs from every theme colour, so the karaoke fill and
        // the flow are distinguishable from the surrounding text.
        let bs = BlockStyle {
            colors: &theme,
            base: theme.bg,
            border: &crate::config::BorderConfig::default(),
            tick: flow_ticks,
        };
        let player = PlaybackState {
            current_song: Some(std::sync::Arc::new(ncm_api::SongInfo {
                id: 1,
                name: "test".into(),
                singer: String::new(),
                artist_id: 0,
                album: String::new(),
                album_id: 0,
                pic_url: String::new(),
                duration: 60_000,
                mv: 0,
                copyright: ncm_api::SongCopyright::Free,
                local_path: None,
            })),
            lyrics: Some(lyrics()),
            translated_lyrics: translated,
            position_secs,
            ..PlaybackState::default()
        };

        let mut state = LyricsState::default();
        let start = Instant::now();
        state.flow(true, 5_000.0, start);
        if flow_ticks > 0 {
            state.flow(true, 5_000.0, start + Duration::from_millis(500 * flow_ticks));
        }

        let mut terminal = Terminal::new(TestBackend::new(60, 12)).expect("backend");
        terminal
            .draw(|f| {
                draw(
                    f,
                    &player,
                    &bs,
                    &LyricsConfig {
                        style,
                        gradient: GradientPreset::Rainbow,
                        ktv_color,
                        show_translation,
                        title: "LYRICS",
                    },
                    &mut state,
                    &PanesConfig::default(),
                    &mut Dividers::default(),
                    f.area(),
                );
            })
            .expect("draw");
        terminal.backend().buffer().clone()
    }

    fn symbols(buffer: &Buffer) -> Vec<String> {
        let mut out = Vec::new();
        for y in 0..buffer.area.height {
            let row: String = (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect();
            let row = row.trim().to_string();
            if !row.is_empty() {
                out.push(row);
            }
        }
        out
    }

    fn colored_cells(buffer: &Buffer) -> Vec<Color> {
        let mut colors = Vec::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                if cell.symbol().trim().is_empty() {
                    continue;
                }
                if let Color::Rgb(..) = cell.fg {
                    colors.push(cell.fg);
                }
            }
        }
        colors
    }

    /// Lines are 5s apart from 0, so 22s is inside line4 (20s..25s).
    #[test]
    fn every_style_shows_the_current_line() {
        for style in LyricStyle::ALL {
            let shown = symbols(&render(style, 22.0, 0));
            assert!(
                shown.iter().any(|row| row.contains("line4")),
                "{style:?} did not show the current line: {shown:?}"
            );
        }
    }

    /// One line at a time means exactly that: no neighbouring lyric is on screen.
    #[test]
    fn one_line_shows_only_the_current_line() {
        let shown = symbols(&render(LyricStyle::OneLine, 22.0, 0));
        let lyric_rows: Vec<&String> = shown.iter().filter(|r| r.contains("line")).collect();
        assert_eq!(lyric_rows.len(), 1, "{shown:?}");
        assert!(lyric_rows[0].contains("line4"));
    }

    /// The window style keeps the neighbours, which is what makes it a window.
    #[test]
    fn the_window_shows_the_neighbouring_lines() {
        let shown = symbols(&render(LyricStyle::Window, 22.0, 0));
        assert!(shown.iter().any(|r| r.contains("line3")), "{shown:?}");
        assert!(shown.iter().any(|r| r.contains("line5")), "{shown:?}");
    }

    /// Nothing is highlighted in the plain style: every glyph is the theme's text colour,
    /// where the karaoke window colours the line with the gradient.
    #[test]
    fn the_plain_style_leaves_every_line_in_the_text_colour() {
        let theme = Theme::default();
        let plain = render(LyricStyle::Plain, 22.0, 0);
        // Inside the frame: the border and its title are drawn in the accent colour.
        for y in 1..plain.area.height - 1 {
            for x in 1..plain.area.width - 1 {
                let cell = &plain[(x, y)];
                if cell.symbol().trim().is_empty() {
                    continue;
                }
                assert_eq!(
                    cell.fg, theme.text,
                    "({x},{y}) came out {:?} instead of the plain text colour",
                    cell.fg
                );
            }
        }

        let window = render(LyricStyle::Window, 22.0, 0);
        assert!(
            colored_cells(&window)
                .iter()
                .any(|c| *c != theme.text && *c != theme.muted && *c != theme.border),
            "the karaoke window should paint the gradient"
        );
    }

    /// The flow style animates: the same line is coloured differently once the phase has moved
    /// on, and the neighbouring lines carry the gradient too.
    #[test]
    fn the_flow_style_moves_with_the_phase() {
        let first = render(LyricStyle::Flow, 22.0, 0);
        let later = render(LyricStyle::Flow, 22.0, 4);
        assert_ne!(
            colored_cells(&first),
            colored_cells(&later),
            "the flow did not move between frames"
        );

        let rows = symbols(&later);
        assert!(
            rows.iter().any(|r| r.contains("line3")) && rows.iter().any(|r| r.contains("line5")),
            "the flow keeps the window: {rows:?}"
        );
    }
}

#[cfg(test)]
mod panel_tests {
    use std::collections::HashSet;

    use ncm_api::SongInfo;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
    use ratatui_image::picker::Picker;

    use super::{tests::lyrics, *};
    use crate::{
        config::BorderConfig,
        state::mv::{self, fixtures},
    };

    /// The page's title, so the tests' `inner` is the one `draw` computes from the same block.
    const TITLE: &str = "LYRICS";

    /// A song with an MV, sung on the page the tests render.
    fn player() -> PlaybackState {
        PlaybackState {
            current_song: Some(std::sync::Arc::new(SongInfo {
                id: 1,
                name: "test".into(),
                singer: String::new(),
                artist_id: 0,
                album: String::new(),
                album_id: 0,
                pic_url: String::new(),
                duration: 60_000,
                mv: 7,
                copyright: ncm_api::SongCopyright::Free,
                local_path: None,
            })),
            lyrics: Some(lyrics()),
            position_secs: 22.0,
            ..PlaybackState::default()
        }
    }

    /// Render `player`'s page at `width × height`, and hand back the frame with the area `draw`
    /// divides between the lyrics and the poster — computed the way it does, so an assertion
    /// about the panel is an assertion about the column it really gets.
    fn render(
        player: &PlaybackState,
        style: LyricStyle,
        width: u16,
        height: u16,
    ) -> (Buffer, Rect) {
        let (buffer, inner, _) = render_with(player, style, width, height, &mut Dividers::default());

        (buffer, inner)
    }

    /// The same, handing back the dividers the page registered — a pane the mouse cannot land on
    /// is a pane that cannot be dragged.
    fn render_with(
        player: &PlaybackState,
        style: LyricStyle,
        width: u16,
        height: u16,
        dividers: &mut Dividers,
    ) -> (Buffer, Rect, Dividers) {
        let theme = Theme::default();
        let bs = BlockStyle {
            colors: &theme,
            base: theme.bg,
            border: &BorderConfig::default(),
            tick: 0,
        };
        let frame = Rect::new(0, 0, width, height);
        let inner = CornerBlock::from_color(&bs, bs.base)
            .title(TITLE, bs.colors)
            .inner(frame);

        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("backend");
        terminal
            .draw(|f| {
                draw(
                    f,
                    player,
                    &bs,
                    &LyricsConfig {
                        style,
                        gradient: GradientPreset::Rainbow,
                        ktv_color: Color::Rgb(77, 166, 255),
                        show_translation: true,
                        title: TITLE,
                    },
                    &mut LyricsState::default(),
                    &PanesConfig::default(),
                    dividers,
                    f.area(),
                );
            })
            .expect("draw");
        (
            terminal.backend().buffer().clone(),
            inner,
            std::mem::take(dividers),
        )
    }

    /// Every cell of the frame as it will be painted — the symbol and both colours — so two
    /// frames compare the way the terminal would receive them, not just as text.
    fn frame_text(buffer: &Buffer) -> String {
        let mut out = String::with_capacity(buffer.content.len() * 8);
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                out.push_str(cell.symbol());
                out.push_str(&format!("{:?}{:?}|", cell.fg, cell.bg));
            }
            out.push('\n');
        }
        out
    }

    /// The rows of `area` as text, which is what a reader sees in that column.
    fn rows_in(buffer: &Buffer, area: Rect) -> Vec<String> {
        (area.top()..area.bottom())
            .map(|y| {
                (area.left()..area.right())
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }

    /// Whether any of `rows` spells `text`. The comparison ignores spacing, because a cell grid
    /// writes the gap behind a wide glyph as a cell of its own: `MV标题` comes back as
    /// `MV 标 题`.
    fn spells(rows: &[String], text: &str) -> bool {
        let squeezed = |s: &str| -> String { s.chars().filter(|c| !c.is_whitespace()).collect() };
        let wanted = squeezed(text);
        rows.iter().any(|row| squeezed(row).contains(&wanted))
    }

    /// The positions of the blurb's characters on the frame. Nothing else on the page spells
    /// them — the lyrics are `line0`…, the facts are digits — so where they are is where the
    /// blurb was drawn.
    fn blurb_cells(buffer: &Buffer) -> Vec<(u16, u16)> {
        let spelled: HashSet<char> = fixtures::DESC
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let mut cells = Vec::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                if buffer[(x, y)]
                    .symbol()
                    .chars()
                    .any(|c| spelled.contains(&c))
                {
                    cells.push((x, y));
                }
            }
        }
        cells
    }

    /// How many cells of `area` carry a colour the page's own text never uses. Every string is
    /// theme-coloured, so those cells are the poster's pixels.
    fn poster_cells(buffer: &Buffer, area: Rect, theme: &Theme) -> usize {
        let palette = [
            theme.bg,
            theme.surface,
            theme.text,
            theme.accent,
            theme.muted,
            theme.border,
            theme.error,
            Color::Reset,
            Color::White,
        ];
        (area.top()..area.bottom())
            .flat_map(|y| (area.left()..area.right()).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let cell = &buffer[(x, y)];
                !palette.contains(&cell.fg) || !palette.contains(&cell.bg)
            })
            .count()
    }

    /// The panel takes a column off the right edge only when there is one to draw and the page
    /// can hold it; every other answer is the lyrics area untouched, which is what keeps the
    /// page the page it was. The column is as wide as the page's height can afford, up to the
    /// tallest poster there is.
    #[test]
    fn the_panel_is_only_handed_a_column_when_the_page_can_hold_it() {
        let tall = Rect::new(0, 0, 100, 40);
        let (lyrics, panel) = panel_split(tall, &PanesConfig::default(), true);
        let panel = panel.expect("a 100×40 page holds the panel");
        assert_eq!(
            panel.width,
            MAX_POSTER_ROWS * 2,
            "a page with rows to spare gets the tallest poster"
        );
        assert_eq!(panel, Rect::new(100 - panel.width, 0, panel.width, 40));
        assert_eq!(
            lyrics.width,
            100 - panel.width - PANEL_GAP,
            "the lyrics keep everything the poster and the gap do not take"
        );

        let small = Rect::new(0, 0, 100, MIN_PANEL_HEIGHT);
        let (lyrics, panel) = panel_split(small, &PanesConfig::default(), true);
        let panel = panel.expect("the shortest page that holds the panel does hold it");
        assert_eq!(
            panel.width,
            MIN_POSTER_ROWS * 2,
            "a shorter page gets a smaller poster rather than none"
        );
        assert_eq!(lyrics.width, 100 - panel.width - PANEL_GAP);

        assert_eq!(
            panel_split(tall, &PanesConfig::default(), false),
            (tall, None),
            "a song with no poster is handed no column"
        );
        let short = Rect::new(0, 0, 100, MIN_PANEL_HEIGHT - 1);
        assert_eq!(panel_split(short, &PanesConfig::default(), true), (short, None));
        let narrow = Rect::new(
            0,
            0,
            MAX_POSTER_ROWS * 2 + PANEL_GAP + MIN_LYRICS_WIDTH - 1,
            40,
        );
        assert_eq!(panel_split(narrow, &PanesConfig::default(), true), (narrow, None));
        assert_eq!(
            panel_split(Rect::new(0, 0, 60, 12), &PanesConfig::default(), true),
            (Rect::new(0, 0, 60, 12), None),
            "the page the other tests draw on"
        );
    }

    /// The MV column is a pane like the sidebar: `[panes] mv` is its width, collapsing it hands
    /// the whole page to the lyrics, and a width the page cannot hold is clamped rather than
    /// taken out of the lyrics — a config written on a wide terminal has to stay usable on a
    /// narrow one.
    #[test]
    fn the_mv_column_is_a_pane() {
        let page = Rect::new(0, 0, 100, 30);
        let auto = panel_split(page, &PanesConfig::default(), true)
            .1
            .expect("the default is the width the page can spare");
        assert_eq!(auto.width, poster_rows(page.height) * 2);

        let dragged = PanesConfig {
            mv: 30,
            ..PanesConfig::default()
        };
        let panel = panel_split(page, &dragged, true)
            .1
            .expect("a dragged width is used");
        assert_eq!(panel.width, 30);
        assert_eq!(panel.right(), page.right(), "the column stays on the right");

        let collapsed = PanesConfig {
            collapsed: vec![Pane::Mv],
            ..PanesConfig::default()
        };
        assert_eq!(
            panel_split(page, &collapsed, true),
            (page, None),
            "a collapsed pane is not drawn, whatever the page could hold"
        );

        // Wider than the page: clamped, so the lyrics keep their columns.
        let too_wide = PanesConfig {
            mv: 200,
            ..PanesConfig::default()
        };
        let (lyrics, panel) = panel_split(page, &too_wide, true);
        assert!(panel.is_some(), "a clamped column is still a column");
        assert!(
            lyrics.width >= MIN_LYRICS_WIDTH,
            "the lyrics keep their columns: {lyrics:?}"
        );
    }

    /// The column's edge is the divider the mouse lands on, and dragging it left has to make the
    /// column wider — the pane is on the right, so its edge works the other way round from the
    /// sidebar's.
    #[tokio::test]
    async fn the_mv_edge_is_draggable_and_grows_leftwards() {
        let _turn = fixtures::turn().await;
        mv::clear();
        assert!(mv::install(
            mv::generation(),
            fixtures::panel(&Picker::halfblocks())
        ));

        let mut dividers = Dividers::default();
        let (_, inner, dividers) =
            render_with(&player(), LyricStyle::Window, 100, 30, &mut dividers);
        let (_, panel) = panel_split(inner, &PanesConfig::default(), true);
        let panel = panel.expect("a 100×30 page holds the panel");

        let divider = dividers
            .iter()
            .find(|divider| divider.pane == Pane::Mv)
            .expect("the page registers the MV's edge");
        assert_eq!(divider.axis, Axis::Columns);
        assert_eq!(divider.rect.x, panel.x - 1, "the edge is the column beside it");
        assert_eq!(divider.rect.height, panel.height);
        assert_eq!(
            divider.size_for(30, -4),
            34,
            "dragging the edge left makes the column wider"
        );

        mv::clear();
    }

    /// With a poster in the slot, the panel's column paints the poster and writes the MV's own
    /// facts in it, while the lyrics stay in the columns they had.
    #[tokio::test]
    async fn a_panel_paints_its_poster_and_its_facts_in_its_own_column() {
        let _turn = fixtures::turn().await;
        let theme = Theme::default();
        mv::clear();
        assert!(mv::install(
            mv::generation(),
            fixtures::panel(&Picker::halfblocks())
        ));

        let (buffer, inner) = render(&player(), LyricStyle::Window, 100, 30);
        let (lyrics_area, panel) = panel_split(inner, &PanesConfig::default(), true);
        let panel = panel.expect("a 100×30 page holds the panel");

        let poster_rows = panel.width / 2;
        let painted = poster_cells(&buffer, panel, &theme);
        assert!(
            painted >= panel.width as usize * poster_rows as usize / 2,
            "the poster should paint its half of the column, {painted} cells did"
        );

        let rows = rows_in(&buffer, panel);
        for fact in [
            fixtures::NAME,
            fixtures::ARTIST,
            &format_duration(fixtures::DURATION_MS),
            fixtures::PUBLISH_TIME,
        ] {
            assert!(
                spells(&rows, fact),
                "{fact:?} is not in the panel's column: {rows:?}"
            );
        }
        assert!(
            !spells(&rows, fixtures::DESC),
            "the blurb is a paragraph, not one line: {rows:?}"
        );
        assert!(
            blurb_cells(&buffer)
                .iter()
                .all(|&(x, _)| { x >= panel.left() && x < panel.right() }),
            "the blurb must stay in the panel's column"
        );

        let lyrics_rows = rows_in(&buffer, lyrics_area);
        assert!(
            spells(&lyrics_rows, "line4"),
            "the lyrics still draw beside the poster: {lyrics_rows:?}"
        );

        mv::clear();
    }

    /// The rows under the facts are all the blurb gets: what it holds is drawn there, and the
    /// rest of it is dropped rather than written past the panel.
    #[tokio::test]
    async fn the_blurb_is_cut_to_the_rows_the_panel_has_left() {
        let _turn = fixtures::turn().await;
        mv::clear();
        assert!(mv::install(
            mv::generation(),
            fixtures::panel(&Picker::halfblocks())
        ));

        // The shortest page that still holds the panel: poster, facts and one row of blurb.
        let (buffer, inner) = render(&player(), LyricStyle::Window, 100, MIN_PANEL_HEIGHT + 2);
        let (_, panel) = panel_split(inner, &PanesConfig::default(), true);
        let panel = panel.expect("this is the page the panel is measured against");

        let drawn = blurb_cells(&buffer).len();
        let whole = fixtures::DESC
            .chars()
            .filter(|c| !c.is_whitespace())
            .count();
        assert!(drawn > 0, "the row the blurb has should hold some of it");
        assert!(
            drawn < whole,
            "the blurb should be cut, not all {whole} characters of it drawn"
        );

        let rows = rows_in(&buffer, panel);
        assert!(
            spells(&rows, fixtures::PUBLISH_TIME),
            "the facts stay when the blurb is cut: {rows:?}"
        );

        mv::clear();
    }

    /// All four presentations keep working beside the panel: the lyrics stay in their column and
    /// the poster in its own, whichever one is drawn.
    #[tokio::test]
    async fn every_style_draws_beside_the_panel() {
        let _turn = fixtures::turn().await;
        let theme = Theme::default();
        mv::clear();
        assert!(mv::install(
            mv::generation(),
            fixtures::panel(&Picker::halfblocks())
        ));

        for style in LyricStyle::ALL {
            let (buffer, inner) = render(&player(), style, 100, 30);
            let (lyrics_area, panel) = panel_split(inner, &PanesConfig::default(), true);
            let panel = panel.expect("a 100×30 page holds the panel");
            assert!(
                spells(&rows_in(&buffer, panel), fixtures::NAME),
                "{style:?} drew over the panel"
            );
            assert!(
                spells(&rows_in(&buffer, lyrics_area), "line4"),
                "{style:?} lost the line being sung"
            );
            assert!(
                poster_cells(&buffer, panel, &theme) > 0,
                "{style:?} painted the poster out of its column"
            );
        }

        mv::clear();
    }

    /// The poster belongs to the song, not to its lyrics: while the lyrics are still on their
    /// way — or when the song has none to fetch — the panel is drawn, and it is the page there
    /// is to look at.
    #[tokio::test]
    async fn the_panel_is_drawn_before_the_lyrics_arrive() {
        let _turn = fixtures::turn().await;
        mv::clear();
        assert!(mv::install(
            mv::generation(),
            fixtures::panel(&Picker::halfblocks())
        ));

        let mut pending = player();
        pending.lyrics = None;
        let (buffer, inner) = render(&pending, LyricStyle::Window, 100, 30);
        let (_, panel) = panel_split(inner, &PanesConfig::default(), true);
        let panel = panel.expect("the page holds the panel");
        assert!(
            spells(&rows_in(&buffer, panel), fixtures::NAME),
            "the panel is not drawn while the lyrics are loading"
        );

        mv::clear();
    }

    /// A page too small for the panel is byte for byte the page it was before the panel
    /// existed, in every style: the poster waiting in the slot changes nothing.
    #[tokio::test]
    async fn a_page_too_small_for_the_panel_is_the_page_without_one() {
        let _turn = fixtures::turn().await;
        mv::clear();
        let before: Vec<String> = LyricStyle::ALL
            .iter()
            .map(|style| frame_text(&render(&player(), *style, 60, 12).0))
            .collect();

        assert!(mv::install(
            mv::generation(),
            fixtures::panel(&Picker::halfblocks())
        ));
        for (style, page) in LyricStyle::ALL.iter().zip(before) {
            assert_eq!(
                frame_text(&render(&player(), *style, 60, 12).0),
                page,
                "{style:?} drew a different page with a poster in the slot"
            );
        }

        mv::clear();
    }

    /// The same is true of a page with no poster at all: nothing of the MV reaches it — not one
    /// of its words, and not its pixels.
    #[tokio::test]
    async fn a_page_without_a_panel_draws_none_of_the_mv() {
        let _turn = fixtures::turn().await;
        let theme = Theme::default();
        mv::clear();

        // The plain style paints no gradient, so the only colours on this page are the theme's:
        // any other colour in the frame would be a pixel of a poster that should not be there.
        let (buffer, inner) = render(&player(), LyricStyle::Plain, 100, 30);
        assert_eq!(
            panel_split(inner, &PanesConfig::default(), false),
            (inner, None),
            "the lyrics keep the whole page"
        );

        let rows = rows_in(&buffer, inner);
        for fact in [fixtures::NAME, fixtures::ARTIST, fixtures::PUBLISH_TIME] {
            assert!(
                !spells(&rows, fact),
                "{fact:?} is on a page that has no panel: {rows:?}"
            );
        }
        assert!(blurb_cells(&buffer).is_empty());
        assert_eq!(
            poster_cells(&buffer, inner, &theme),
            0,
            "a page without a panel paints nothing but the theme"
        );
    }
}
