use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{LineGauge, Paragraph},
};
use ratatui_image::{Resize, StatefulImage};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    config::{PlayerbarConfig, Theme, symbols},
    playback::{PlaybackState, mode_icon},
    ui::{gradient_line_gauge::GradientLineGauge, spinner::Spinner},
    utils::{format_duration_into, time::format_duration},
};

pub(super) fn draw_song_info(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    if let Some(song) = &player.current_song {
        let like_icon = if player.liked { "\u{f004}" } else { "\u{f08a}" };
        let like_color = if player.liked {
            colors.accent
        } else {
            colors.muted
        };
        let info_lines = vec![
            Line::from(vec![
                Span::styled("\u{266a} ", Style::default().fg(colors.accent)),
                Span::styled(
                    &song.name,
                    Style::default()
                        .fg(colors.text)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled(like_icon, Style::default().fg(like_color)),
                Span::raw(" "),
                Span::styled(
                    &song.singer,
                    Style::default()
                        .fg(colors.muted)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
        ];
        f.render_widget(Paragraph::new(info_lines), area);
    } else {
        let idle = Line::from(Span::styled("未在播放", Style::default().fg(colors.muted)));
        f.render_widget(Paragraph::new(idle), area);
    }
}

pub(super) fn draw_controls(
    f: &mut Frame,
    player: &PlaybackState,
    colors: &Theme,
    area: Rect,
    is_default: bool,
) {
    let play_icon = if player.paused || !player.playing {
        "\u{f040a}"
    } else {
        "\u{f03e4}"
    };
    let alignment = if is_default {
        Alignment::Center
    } else {
        Alignment::Left
    };
    let controls = Line::from(vec![
        Span::styled("\u{f049}", Style::default().fg(colors.muted)),
        Span::raw("   "),
        Span::styled(
            play_icon,
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled("\u{f050}", Style::default().fg(colors.muted)),
    ])
    .alignment(alignment);
    f.render_widget(Paragraph::new(controls), area);
}

pub(super) fn draw_mode_icon(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    let (icon, _) = mode_icon(&player.mode);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            icon,
            Style::default().fg(colors.accent),
        )))
        .alignment(Alignment::Right),
        area,
    );
}

pub(super) fn draw_spinner(f: &mut Frame, tick: u64, colors: &Theme, area: Rect) {
    f.render_widget(
        Spinner::new(tick)
            .active_color(Style::default().fg(colors.accent))
            .inactive_color(Style::default().fg(colors.surface)),
        area,
    );
}

pub(super) fn draw_current_time(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    if let Some(song) = &player.current_song {
        let cur_ms = (player.progress * song.duration as f64) as u64;
        let mut buf = String::with_capacity(8);
        format_duration_into(cur_ms, &mut buf);
        f.render_widget(
            Paragraph::new(buf)
                .style(Style::default().fg(colors.text))
                .alignment(Alignment::Right),
            area,
        );
    }
}

pub(super) fn draw_total_time(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    if let Some(song) = &player.current_song {
        let mut buf = String::with_capacity(8);
        format_duration_into(song.duration, &mut buf);
        f.render_widget(
            Paragraph::new(buf)
                .style(Style::default().fg(colors.text))
                .alignment(Alignment::Right),
            area,
        );
    }
}

pub(super) fn draw_gauge_bar(
    f: &mut Frame,
    player: &PlaybackState,
    colors: &Theme,
    pb: &PlayerbarConfig,
    area: Rect,
) {
    if player.current_song.is_none() {
        return;
    }
    render_gauge(
        f,
        pb,
        colors,
        player.cached,
        player.progress.clamp(0.0, 1.0),
        Line::from(""),
        area,
    );
}

pub(super) fn draw_gauge_with_label(
    f: &mut Frame,
    player: &PlaybackState,
    colors: &Theme,
    pb: &PlayerbarConfig,
    area: Rect,
) {
    let time_buf = if let Some(song) = &player.current_song {
        let cur_ms = (player.progress * song.duration as f64) as u64;
        let mut buf = String::with_capacity(16);
        format_duration_into(cur_ms, &mut buf);
        buf.push_str(" / ");
        buf.push_str(&format_duration(song.duration));
        buf
    } else {
        "00:00 / 00:00".into()
    };
    let ratio = if player.current_song.is_some() {
        player.progress.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let label = Line::from(Span::styled(time_buf, Style::default().fg(colors.text)));
    render_gauge(f, pb, colors, player.cached, ratio, label, area);
}

/// Render a progress gauge using the configured filled/unfilled symbols and
/// colors. Uses `GradientLineGauge` when a gradient preset is configured,
/// otherwise the plain `LineGauge`. `cached` selects the cached-file unfilled
/// color when true.
fn render_gauge(
    f: &mut Frame,
    pb: &PlayerbarConfig,
    colors: &Theme,
    cached: bool,
    ratio: f64,
    label: Line,
    area: Rect,
) {
    let unfilled_color = if cached {
        pb.unfilled_color_cached.as_str()
    } else {
        pb.unfilled_color.as_str()
    };

    if let Some(preset) = pb.gradient_preset {
        let gauge = GradientLineGauge::new(preset)
            .ratio(ratio)
            .label(label)
            .filled_symbol(&pb.filled_symbol)
            .unfilled_symbol(&pb.unfilled_symbol)
            .unfilled_style(Style::default().fg(colors.field_color(unfilled_color)));
        f.render_widget(gauge, area);
    } else {
        let gauge = LineGauge::default()
            .filled_symbol(&pb.filled_symbol)
            .unfilled_symbol(&pb.unfilled_symbol)
            .filled_style(Style::default().fg(colors.field_color(&pb.filled_color)))
            .unfilled_style(Style::default().fg(colors.field_color(unfilled_color)))
            .ratio(ratio)
            .label(label);
        f.render_widget(gauge, area);
    }
}

pub(super) fn draw_song_detail(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    if let Some(song) = &player.current_song {
        let like_icon = if player.liked { "\u{f004}" } else { "\u{f08a}" };
        let like_color = if player.liked {
            colors.accent
        } else {
            colors.muted
        };
        let detail = Line::from(vec![
            Span::styled(like_icon, Style::default().fg(like_color)),
            Span::raw(" "),
            Span::styled(
                &song.singer,
                Style::default()
                    .fg(colors.muted)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .style(Style::default().fg(colors.muted));
        f.render_widget(Paragraph::new(detail), area);
    }
}
/// Frequency bars of what is playing, one cell per column.
pub(super) fn draw_visualizer(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    let bars: Vec<char> = symbols().visualizer_bars.chars().collect();
    let levels = &player.visualizer;
    let columns = area.width as usize;
    if columns == 0 || area.height == 0 || bars.len() < 2 {
        return;
    }

    let mut spans = Vec::with_capacity(columns);
    for column in 0..columns {
        let level = if levels.is_empty() {
            0.0
        } else {
            // Spread the bands over the available width, one band per column.
            let band = (column * levels.len() / columns).min(levels.len() - 1);
            levels[band].clamp(0.0, 1.0)
        };
        let glyph = bars[(level * (bars.len() - 1) as f32).round() as usize];
        let style = if level > 0.0 {
            Style::default().fg(colors.accent)
        } else {
            Style::default().fg(colors.muted)
        };
        spans.push(Span::styled(glyph.to_string(), style));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Dominant-pitch readout: note name with octave plus the detected frequency.
///
/// A narrow area cannot hold the whole label, and truncating it would hide the octave or
/// the frequency, so the text scrolls through the area and wraps around instead.
pub(super) fn draw_pitch(
    f: &mut Frame,
    player: &PlaybackState,
    colors: &Theme,
    tick: u64,
    area: Rect,
) {
    let Some(note) = &player.pitch else {
        return;
    };

    let label = format!("{} {:>4.0}Hz", note.label(), note.frequency);
    let width = UnicodeWidthStr::width(label.as_str());
    let style = Style::default().fg(colors.accent);

    if width <= area.width as usize {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(label, style))).alignment(Alignment::Right),
            area,
        );
        return;
    }

    // One cell per two ticks (~160ms at the loop's rate): fast enough to read, slow
    // enough not to blur into noise.
    let offset = (tick / PITCH_SCROLL_TICKS) as usize;
    let scrolled = scroll_text(
        &format!("{label}{PITCH_SCROLL_GAP}"),
        offset,
        area.width as usize,
    );
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(scrolled, style))).alignment(Alignment::Left),
        area,
    );
}

/// Ticks per scrolled cell of the pitch readout.
const PITCH_SCROLL_TICKS: u64 = 2;
/// Cells of blank between two passes of a scrolling readout.
const PITCH_SCROLL_GAP: &str = "   ";

/// A window of `width` cells into `text`, starting `offset` cells in and wrapping around.
///
/// Widths are measured per character, so a wide glyph occupies the two cells it draws and
/// the window never ends mid-glyph.
pub(super) fn scroll_text(text: &str, offset: usize, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let total: usize = chars
        .iter()
        .map(|c| UnicodeWidthChar::width(*c).unwrap_or(0))
        .sum();
    if total == 0 || width == 0 {
        return String::new();
    }

    let start = offset % total;
    let mut out = String::new();
    let mut used = 0usize;
    let mut index = 0usize;
    // Walk far enough to fill `width` cells, with a hard bound so a text made only of
    // zero-width characters cannot spin here.
    let max_steps = chars.len() * 2 + width;
    while used < width && index < max_steps {
        let ch = chars[(start + index) % chars.len()];
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > width {
            break;
        }
        out.push(ch);
        used += ch_width;
        index += 1;
    }
    out
}

pub(super) fn draw_volume(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    let icon = symbols().volume_icon(player.volume);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            icon,
            Style::default().fg(colors.accent),
        )))
        .alignment(Alignment::Right),
        area,
    );
}

pub(super) fn draw_cover(f: &mut Frame, player: &PlaybackState, colors: &Theme, area: Rect) {
    if player.current_song.is_some() {
        // Try to render real cover image if available
        if let Ok(mut borrow) = player.cover.protocol.lock()
            && let Some(protocol) = borrow.as_mut()
        {
            let image = StatefulImage::new().resize(Resize::Fit(None));
            f.render_stateful_widget(image, area, protocol);
            return;
        }

        // Fallback to placeholder (no border)
        for y in 0..area.height {
            for x in 0..area.width {
                if let Some(cell) = f.buffer_mut().cell_mut((area.x + x, area.y + y)) {
                    cell.set_char('░');
                    cell.set_style(Style::default().fg(colors.surface));
                }
            }
        }

        let icon = "\u{266a}";
        let icon_x = area.x + area.width / 2;
        let icon_y = area.y + area.height / 2;
        if let Some(cell) = f.buffer_mut().cell_mut((icon_x, icon_y)) {
            cell.set_char(icon.chars().next().unwrap_or('♪'));
            cell.set_style(Style::default().fg(colors.accent));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::scroll_text;

    /// The marquee window slides by whole cells, wraps around, and fills the width it was
    /// given — the pitch readout relies on all three to stay readable in a narrow cell.
    #[test]
    fn scroll_text_wraps_and_clips_by_cells() {
        assert_eq!(scroll_text("A4  440Hz", 0, 4), "A4  ");
        assert_eq!(scroll_text("A4  440Hz", 2, 4), "  44");
        assert_eq!(scroll_text("A4  440Hz", 9, 4), "A4  ", "wraps to the start");
        assert_eq!(scroll_text("abc", 2, 6), "cabcab");
        assert_eq!(
            scroll_text("ab", 0, 5),
            "ababa",
            "repeats to fill the width"
        );
    }

    /// A wide glyph takes the two cells it draws and is never cut in half.
    #[test]
    fn scroll_text_never_splits_a_wide_glyph() {
        assert_eq!(scroll_text("中ab", 0, 2), "中");
        assert_eq!(scroll_text("中ab", 1, 2), "ab");
        assert_eq!(scroll_text("中ab", 1, 1), "a");
    }

    #[test]
    fn scroll_text_handles_degenerate_input() {
        assert_eq!(scroll_text("", 0, 4), "");
        assert_eq!(scroll_text("abc", 0, 0), "");
    }
}
