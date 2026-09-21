//! The lyrics page: the song's lyrics, and the MV's poster beside them.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::Line,
    widgets::{Padding, Paragraph},
};

use crate::utils::Named;

use super::{
    BlockStyle,
    block::CornerBlock,
};
use crate::{
    config::{Pane, PanesConfig, lyrics::LyricsConfig},
    layout::{Axis, Divider, Dividers},
    playback::PlaybackState,
    state::{
        lyrics::LyricsState,
        mv,
    },
};

mod panel;
mod styles;
mod view;

use panel::{draw_panel, panel_split};
use styles::presentation;
use view::View;

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

    // The table, not a `match`: a style with no presentation is a failing test rather than a page
    // that draws the window because nobody wrote its arm.
    presentation(style)(f, &view, inner);
}

/// The lyric files the tests draw.
///
/// The page, its styles and its panel all draw the same song, so the file they draw is written
/// once here rather than three times with three chances to drift.
#[cfg(test)]
pub(crate) mod lines {
    use std::time::Duration;

    use crate::playback::LyricLine;

    /// Twelve lines, five seconds apart.
    pub(crate) fn lyrics() -> Vec<LyricLine> {
        (0..12)
            .map(|i| LyricLine {
                time: Duration::from_millis(i * 5_000),
                text: format!("line{i}"),
            })
            .collect()
    }

    /// The same shape, as a translation: what a translated file hands the player.
    pub(crate) fn translated() -> Vec<LyricLine> {
        (0..12)
            .map(|i| LyricLine {
                time: Duration::from_millis(i * 5_000),
                text: format!("译文{i}"),
            })
            .collect()
    }
}
