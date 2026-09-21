//! The five presentations: how much of the song is on screen, and how the line being sung is
//! painted.
//!
//! [`Fill`] is what `window` and `ktv` share — the karaoke sweep — and the difference between them
//! is one colour against the gradient. Nothing here reads the clock: the phase it draws with comes
//! from the state, which is what makes a presentation a function of its inputs.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::view::View;
use crate::config::{LyricStyle, symbols};

/// One of the ways the page can draw: the style it is for, and the function that draws it.
pub(super) struct Presentation {
    pub style: LyricStyle,
    pub draw: fn(&mut Frame, &View<'_>, Rect),
}

/// Every style, and how it is drawn.
///
/// The dispatch reads this table, which is what makes a style whose presentation is missing a
/// *test* failure rather than a style that silently draws the window: adding a variant to
/// `LyricStyle` without a row here leaves `every_style_has_a_presentation` red.
pub(super) static PRESENTATIONS: [Presentation; 5] = [
    Presentation {
        style: LyricStyle::Window,
        draw: |f, view, area| draw_window(f, view, area, Fill::Gradient),
    },
    Presentation {
        style: LyricStyle::OneLine,
        draw: draw_one_line,
    },
    Presentation {
        style: LyricStyle::Ktv,
        draw: |f, view, area| draw_window(f, view, area, Fill::Ktv),
    },
    Presentation {
        style: LyricStyle::Flow,
        draw: draw_flow,
    },
    Presentation {
        style: LyricStyle::Plain,
        draw: draw_plain,
    },
];

/// How a style is drawn.
pub(super) fn presentation(style: LyricStyle) -> fn(&mut Frame, &View<'_>, Rect) {
    PRESENTATIONS
        .iter()
        .find(|presentation| presentation.style == style)
        .map(|presentation| presentation.draw)
        .expect("every style has a presentation")
}

/// How the line being sung is painted in the scrolling window.

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Fill {
    /// The gradient: the sung part forward, the rest of the line reversed.
    Gradient,
    /// One flat colour over the sung part, the way a karaoke screen covers a word.
    Ktv,
}

/// The default: a scrolling window of the lines around the current one.
pub(super) fn draw_window(f: &mut Frame, view: &View<'_>, inner: Rect, fill: Fill) {
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
            // The theme's own translation look, so a palette can say how a translation reads
            // rather than leaving it to the code that draws it.
            view.colors.looks().lyric_translation.style()
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
pub(super) fn draw_one_line(f: &mut Frame, view: &View<'_>, inner: Rect) {
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
pub(super) fn draw_flow(f: &mut Frame, view: &View<'_>, inner: Rect) {
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
pub(super) fn draw_plain(f: &mut Frame, view: &View<'_>, inner: Rect) {
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

    use super::super::{
        draw,
        lines::{lyrics, translated as translated_lyrics},
    };
    use super::*;
    use crate::utils::Named;
    use crate::{
        config::{PanesConfig, Theme, lyrics::LyricsConfig},
        layout::Dividers,
        playback::{LyricLine, PlaybackState},
        state::lyrics::LyricsState,
        ui::BlockStyle,
        utils::GradientPreset,
    };

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
        for style in LyricStyle::ALL.iter().copied() {
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

    /// Every style has exactly one presentation, and no row is left over.
    ///
    /// The page dispatches through [`PRESENTATIONS`], so a style with no row would be a style the
    /// config can be set to and the page cannot draw — before the table existed, the missing arm
    /// of a `match` drew the window instead, silently.
    #[test]
    fn every_style_has_a_presentation() {
        for style in LyricStyle::ALL.iter().copied() {
            assert_eq!(
                PRESENTATIONS
                    .iter()
                    .filter(|presentation| presentation.style == style)
                    .count(),
                1,
                "{} has no presentation, or more than one",
                style.name()
            );
        }
        assert_eq!(
            PRESENTATIONS.len(),
            LyricStyle::ALL.len(),
            "a presentation is for a style that no longer exists"
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

