use std::cell::Cell;

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Padding, Paragraph},
};

use super::{BlockStyle, block::CornerBlock};
use crate::{
    config::{LyricStyle, Theme},
    playback::{LyricLine, PlaybackState},
    utils::GradientPreset,
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

    fn lyrics() -> Vec<LyricLine> {
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
                copyright: ncm_api::SongCopyright::Free,
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
