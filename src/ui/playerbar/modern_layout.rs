use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
};

use super::{LayoutArea, Playerbar, widgets};
use crate::{
    config::{PlayerbarConfig, Theme},
    playback::PlaybackState,
};

pub(super) struct ModernLayout;

impl Playerbar for ModernLayout {
    fn layout(&self, area: Rect, config: &PlayerbarConfig, is_sixel: bool) -> LayoutArea {
        let cols = Layout::horizontal([
            if config.visible.cover {
                Constraint::Length(8)
            } else {
                Constraint::Length(0)
            },
            Constraint::Min(20),
        ])
        .spacing(1)
        .split(area);

        let cover_area = cols[0];

        // This layout fills every row, so the spectrum takes the bottom row of the cover
        // column — and only when it is enabled, so nobody else loses a row of art.
        let wants_bars = config.visible.visualizer;
        let cover_height = (if is_sixel && area.height >= 5 { 4 } else { 3 }).min(area.height);
        let cover_height = if wants_bars && cover_height > 1 {
            cover_height - 1
        } else {
            cover_height
        };
        let spectrum_row = if wants_bars && cover_height < area.height {
            Rect {
                y: area.y + cover_height,
                height: 1,
                ..cover_area
            }
        } else {
            Rect::default()
        };
        let cover_area = Rect {
            y: area.y,
            height: cover_height,
            ..cover_area
        };

        let right_rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .horizontal_margin(1)
        .split(cols[1]);

        let progress_cols = Layout::horizontal([
            Constraint::Length(6),
            Constraint::Min(10),
            Constraint::Length(6),
        ])
        .split(right_rows[0]);

        // Middle: song_info(left) | spinner(right)
        let middle_cols = Layout::horizontal([Constraint::Min(10), Constraint::Length(8)])
            .flex(Flex::SpaceBetween)
            .split(right_rows[1]);

        // Bottom: song_detail(left) | controls(center) | mode(right)
        let bottom_cols = Layout::horizontal([
            Constraint::Length(15),
            Constraint::Length(20),
            Constraint::Length(6),
        ])
        .flex(Flex::SpaceBetween)
        .split(right_rows[2]);

        let vol_mode_cols = Layout::horizontal([Constraint::Length(3), Constraint::Length(3)])
            .split(bottom_cols[2]);

        LayoutArea {
            cover: cover_area,
            progress_time_left: progress_cols[0],
            progress_bar: progress_cols[1],
            progress_time_right: progress_cols[2],
            song_info: middle_cols[0],
            spinner: middle_cols[1],
            song_detail: bottom_cols[0],
            controls: bottom_cols[1],
            controls_centered: false,
            volume: vol_mode_cols[0],
            mode_icon: vol_mode_cols[1],
            visualizer: spectrum_row,
            // The cell beside the song info is idle except while seeking, so the readout
            // lives there and scrolls through it.
            pitch: middle_cols[1],
            ..Default::default()
        }
    }

    fn render(
        &self,
        f: &mut Frame,
        player: &PlaybackState,
        colors: &Theme,
        tick: u64,
        config: &PlayerbarConfig,
        layout: &LayoutArea,
    ) {
        if config.visible.cover && layout.cover.width > 0 {
            widgets::draw_cover(f, player, colors, layout.cover);
        }
        // The bottom row of the cover column is the spectrum's, when it has one.
        if config.visible.visualizer && layout.visualizer.width > 0 {
            widgets::draw_visualizer(f, player, colors, layout.visualizer);
        }

        widgets::draw_current_time(f, player, colors, layout.progress_time_left);
        widgets::draw_gauge_bar(f, player, colors, config, layout.progress_bar);
        widgets::draw_total_time(f, player, colors, layout.progress_time_right);

        widgets::draw_song_info(f, player, colors, layout.song_info);
        if player.seeking && config.visible.spinner && layout.spinner.width > 0 {
            widgets::draw_spinner(f, tick, colors, layout.spinner);
        } else if config.visible.pitch && layout.pitch.width > 0 {
            widgets::draw_pitch(f, player, colors, tick, layout.pitch);
        }

        widgets::draw_song_detail(f, player, colors, layout.song_detail);
        widgets::draw_controls(f, player, colors, layout.controls, layout.controls_centered);
        if config.visible.volume && layout.volume.width > 0 {
            widgets::draw_volume(f, player, colors, layout.volume);
        }
        if config.visible.mode_icon {
            widgets::draw_mode_icon(f, player, colors, layout.mode_icon);
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::*;

    /// This layout fills every row with something, so the spectrum can only come from the
    /// cover column — and only when it is enabled, so nobody else loses a row of art.
    #[test]
    fn the_spectrum_takes_a_row_from_the_cover_when_enabled() {
        let area = Rect::new(0, 0, 80, 3);
        let mut config = PlayerbarConfig::default();
        let layout = ModernLayout.layout(area, &config, false);
        assert_eq!(layout.cover.height, 3, "the cover keeps the full height");
        assert_eq!(layout.visualizer.height, 0, "nothing is given up unasked");

        config.visible.visualizer = true;
        let layout = ModernLayout.layout(area, &config, false);
        assert_eq!(layout.cover.height, 2);
        assert_eq!(layout.visualizer.height, 1);
        assert_eq!(layout.visualizer.y, layout.cover.y + layout.cover.height);
        assert_eq!(layout.visualizer.width, layout.cover.width);
        assert_eq!(layout.visualizer.x, layout.cover.x);
        // The readout has a cell of its own here, so it never has to share that row.
        assert_eq!(layout.pitch.height, 1);
        assert!(
            layout.pitch.width > 0,
            "the pitch cell is the idle one beside the song info"
        );
    }
}
