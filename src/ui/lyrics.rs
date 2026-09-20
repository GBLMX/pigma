use std::cell::Cell;

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
    config::{LyricStyle, Theme},
    playback::{LyricLine, PlaybackState},
    state::mv::{self, MvPanel},
    utils::{GradientPreset, format::clip_long_text, format_duration},
};

thread_local! {
    static LAST_CUR: Cell<usize> = const { Cell::new(0) };
}

/// Find the current lyric index — incremental forward scan, O(1) amortized.
fn find_current_line(lyrics: &[LyricLine], cur_ms: f64) -> usize {
    LAST_CUR.with(|last| {
        let mut cur = last.get();
        // Reset if lyrics changed (new song)
        if cur >= lyrics.len() {
            cur = 0;
        }
        // Advance forward from last position
        while cur + 1 < lyrics.len() && lyrics[cur + 1].time.as_millis() as f64 <= cur_ms {
            cur += 1;
        }
        // Only scan backward if we overshot (user seeked back)
        if cur > 0 && lyrics[cur].time.as_millis() as f64 > cur_ms {
            cur = lyrics
                .iter()
                .rposition(|l| l.time.as_millis() as f64 <= cur_ms)
                .unwrap_or(0);
        }
        last.set(cur);
        cur
    })
}

/// Everything the four presentations share: the song's lyrics, where the recording is, and
/// the colours and gradient to draw them with.
struct View<'a> {
    lyrics: &'a [LyricLine],
    translated: Option<&'a [LyricLine]>,
    cur: usize,
    cur_ms: f64,
    colors: &'a Theme,
    gradient: GradientPreset,
    /// Free-running frame counter, used as the clock for the animated styles.
    tick: u64,
}

pub(super) fn draw(
    f: &mut Frame,
    player: &PlaybackState,
    bs: &BlockStyle<'_>,
    gradient: GradientPreset,
    style: LyricStyle,
    title: &str,
    area: Rect,
) {
    let colors = bs.colors;
    let block = CornerBlock::from_color(bs, bs.colors.bg).title(title, bs.colors);
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
    let (inner, panel_area) = panel_split(inner, slot.is_some());
    if let Some(panel_area) = panel_area
        && let Some(panel) = slot.as_mut()
    {
        draw_panel(f, panel_area, panel, colors);
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
    let cur = find_current_line(lyrics, cur_ms);

    let view = View {
        lyrics,
        translated: player
            .translated_lyrics
            .as_deref()
            .filter(|t| !t.is_empty()),
        cur,
        cur_ms,
        colors,
        gradient,
        tick: bs.tick,
    };

    match style {
        LyricStyle::Window => draw_window(f, &view, inner),
        LyricStyle::OneLine => draw_one_line(f, &view, inner),
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

/// Divide `inner` between the lyrics and the poster's column, which is the rightmost one.
///
/// The poster is as tall as the page can spare, up to [`MAX_POSTER_ROWS`], and the column is
/// twice that: a page with rows to give gets the big poster, and a shorter one gets a smaller
/// poster rather than none.
///
/// Returns the lyrics area and `None` when there is no panel to draw, or no room for one: the
/// lyrics then keep the whole page, which is what makes a page without a poster — and a page too
/// small for one — exactly the page this drew before the panel existed.
fn panel_split(inner: Rect, has_panel: bool) -> (Rect, Option<Rect>) {
    if !has_panel || inner.height < MIN_PANEL_HEIGHT {
        return (inner, None);
    }

    let width = poster_rows(inner.height) * 2;
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

/// The default: a scrolling window of the lines around the current one.
fn draw_window(f: &mut Frame, view: &View<'_>, inner: Rect) {
    let h = inner.height as usize;
    let lines_per_lyric = if view.translated.is_some() { 2 } else { 1 };
    let start = view.window_start(h, lines_per_lyric);
    let end = (start + h / lines_per_lyric).min(view.lyrics.len());

    let mut lines: Vec<Line> = Vec::new();
    for i in start..end {
        if i == view.cur {
            lines.push(karaoke_line(
                view,
                i,
                view.lyrics[i + 1].time.as_millis() as f64,
            ));
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

        if let Some(translation) = view.translation(i) {
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
            lines.push(
                Line::from(translation)
                    .style(t_style)
                    .alignment(Alignment::Center),
            );
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

    let next_ms = view
        .lyrics
        .get(view.cur + 1)
        .map(|l| l.time.as_millis() as f64);
    lines.push(karaoke_line(view, view.cur, next_ms.unwrap_or_default()));

    if let Some(translation) = view.translation(view.cur) {
        lines.push(
            Line::from(translation)
                .style(
                    Style::default()
                        .fg(view.colors.text)
                        .add_modifier(Modifier::ITALIC),
                )
                .alignment(Alignment::Center),
        );
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
        lines.push(flow_line(view, view.text(i), keep));

        if let Some(translation) = view.translation(i) {
            let t_keep = if i == view.cur { 0.7 } else { 0.25 };
            lines.push(
                flow_line(view, translation, t_keep)
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

/// The karaoke fill: what has been sung is painted with the gradient, the rest gets the same
/// gradient reversed, and the boundary character marks where the voice is.
fn karaoke_line<'a>(view: &View<'a>, i: usize, next_ms: f64) -> Line<'a> {
    let text = view.text(i);
    let line_ms = view.lyrics[i].time.as_millis() as f64;
    let gradient = view.gradient;

    let seg_dur = (next_ms - line_ms).max(1.0);
    let seg_progress = ((view.cur_ms - line_ms) / seg_dur).clamp(0.0, 1.0);
    let total = text.chars().count();
    let split_at = (total as f64 * seg_progress).floor() as usize;

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
fn flow_line<'a>(view: &View<'a>, text: &'a str, keep: f32) -> Line<'a> {
    let phase = flow_phase(view.tick);
    let total = text.chars().count().max(1);
    let mut line = Line::default();

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

/// How far the gradient has travelled, from the frame counter.
///
/// `tick` advances once per 80ms of wall time (`ui::draw`), not once per frame, so this is a
/// speed rather than a per-frame step and the flow looks the same on a busy and an idle page:
/// 12.5 ticks a second at 0.01 leaves one pass through the palette taking eight seconds.
fn flow_phase(tick: u64) -> f32 {
    const PER_TICK: f32 = 0.01;
    (tick as f32 * PER_TICK).rem_euclid(1.0)
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

    fn render(style: LyricStyle, position_secs: f64, tick: u64) -> Buffer {
        let theme = Theme::default();
        // A line with a gradient that differs from every theme colour, so the karaoke fill and
        // the flow are distinguishable from the surrounding text.
        let bs = BlockStyle {
            colors: &theme,
            border: &crate::config::BorderConfig::default(),
            tick,
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
            position_secs,
            ..PlaybackState::default()
        };

        let mut terminal = Terminal::new(TestBackend::new(60, 12)).expect("backend");
        terminal
            .draw(|f| {
                draw(
                    f,
                    &player,
                    &bs,
                    GradientPreset::Rainbow,
                    style,
                    "LYRICS",
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

    /// The flow style animates: the same line is coloured differently on a later frame, and the
    /// neighbouring lines carry the gradient too.
    #[test]
    fn the_flow_style_moves_with_the_frames() {
        let first = render(LyricStyle::Flow, 22.0, 0);
        let later = render(LyricStyle::Flow, 22.0, 40);
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
mod flow_speed {
    use super::*;

    /// One pass should take about eight seconds, and `tick` counts 80ms steps, so a second is
    /// 12.5 ticks: assert the pace instead of trusting the constant's comment.
    #[test]
    fn one_pass_takes_about_eight_seconds() {
        let ticks_per_second = 1000.0 / 80.0;
        let seconds_per_pass = 1.0 / (flow_phase(1) as f64 * ticks_per_second);
        assert!(
            (7.0..9.0).contains(&seconds_per_pass),
            "one pass takes {seconds_per_pass:.1}s, which is not the pace this is meant to have"
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
        let theme = Theme::default();
        let bs = BlockStyle {
            colors: &theme,
            border: &BorderConfig::default(),
            tick: 0,
        };
        let frame = Rect::new(0, 0, width, height);
        let inner = CornerBlock::from_color(&bs, bs.colors.bg)
            .title(TITLE, bs.colors)
            .inner(frame);

        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("backend");
        terminal
            .draw(|f| {
                draw(
                    f,
                    player,
                    &bs,
                    GradientPreset::Rainbow,
                    style,
                    TITLE,
                    f.area(),
                );
            })
            .expect("draw");
        (terminal.backend().buffer().clone(), inner)
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
        let (lyrics, panel) = panel_split(tall, true);
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
        let (lyrics, panel) = panel_split(small, true);
        let panel = panel.expect("the shortest page that holds the panel does hold it");
        assert_eq!(
            panel.width,
            MIN_POSTER_ROWS * 2,
            "a shorter page gets a smaller poster rather than none"
        );
        assert_eq!(lyrics.width, 100 - panel.width - PANEL_GAP);

        assert_eq!(
            panel_split(tall, false),
            (tall, None),
            "a song with no poster is handed no column"
        );
        let short = Rect::new(0, 0, 100, MIN_PANEL_HEIGHT - 1);
        assert_eq!(panel_split(short, true), (short, None));
        let narrow = Rect::new(
            0,
            0,
            MAX_POSTER_ROWS * 2 + PANEL_GAP + MIN_LYRICS_WIDTH - 1,
            40,
        );
        assert_eq!(panel_split(narrow, true), (narrow, None));
        assert_eq!(
            panel_split(Rect::new(0, 0, 60, 12), true),
            (Rect::new(0, 0, 60, 12), None),
            "the page the other tests draw on"
        );
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
        let (lyrics_area, panel) = panel_split(inner, true);
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
        let (_, panel) = panel_split(inner, true);
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
            let (lyrics_area, panel) = panel_split(inner, true);
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
        let (_, panel) = panel_split(inner, true);
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
            panel_split(inner, false),
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
