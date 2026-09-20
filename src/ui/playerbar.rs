mod default_layout;
mod minimal_layout;
mod modern_layout;
mod widgets;

pub(crate) use widgets::{
    ControlButton, control_rects, like_rect, mode_icon_rect, song_info_like_rect,
};

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    widgets::{Padding, Paragraph},
};

use super::{BlockStyle, block::CornerBlock};
use crate::{
    config::{LayoutType, PlayerbarConfig, PlayerbarVisible, Theme},
    playback::PlaybackState,
};

#[derive(Debug, Clone, Default)]
pub(super) struct LayoutArea {
    pub progress_time_left: Rect,
    pub progress_bar: Rect,
    pub progress_time_right: Rect,
    pub song_info: Rect,
    pub song_detail: Rect,
    pub cover: Rect,
    pub controls: Rect,
    /// Whether the controls row is centred in its area, which is what decides where the
    /// three transport buttons are drawn *and* where they are clickable.
    pub controls_centered: bool,
    pub gauge: Rect,
    pub spinner: Rect,
    pub mode_icon: Rect,
    pub volume: Rect,
    /// Spectrum row; a zero rect when the active layout has no spare row for it.
    pub visualizer: Rect,
    /// Pitch readout cell, for layouts that can give it one of its own instead of sharing
    /// the spectrum row.
    pub pitch: Rect,
}

/// Split the layouts' spare row between the spectrum and the pitch readout, so enabling
/// both does not draw them over each other. Either half is the whole row when only one
/// of the two is visible.
pub(crate) fn spectrum_row(area: Rect, visible: &PlayerbarVisible) -> (Rect, Rect) {
    const PITCH_WIDTH: u16 = 14;

    if visible.visualizer && visible.pitch && area.width > PITCH_WIDTH {
        let cols =
            Layout::horizontal([Constraint::Min(0), Constraint::Length(PITCH_WIDTH)]).split(area);
        (cols[0], cols[1])
    } else {
        (area, area)
    }
}

pub(super) trait Playerbar {
    /// Build the concrete sub-areas from the already-inner area.
    fn layout(&self, area: Rect, config: &PlayerbarConfig, is_sixel: bool) -> LayoutArea;

    fn render(
        &self,
        f: &mut Frame,
        player: &PlaybackState,
        colors: &Theme,
        tick: u64,
        config: &PlayerbarConfig,
        layout: &LayoutArea,
    );

    #[allow(clippy::too_many_arguments)]
    fn draw(
        &self,
        f: &mut Frame,
        player: &PlaybackState,
        tick: u64,
        bs: &BlockStyle<'_>,
        config: &PlayerbarConfig,
        area: Rect,
        is_sixel: bool,
    ) -> LayoutArea {
        let colors = bs.colors;
        let block = CornerBlock::from_color(bs, bs.colors.bg).block_padding(Padding::horizontal(1));
        let inner = block.inner(area);
        f.render_widget(block, area);

        if let Some(err) = &player.error {
            f.render_widget(
                Paragraph::new(format!(" ⚠  {}", err)).style(Style::default().fg(colors.error)),
                inner,
            );
            return LayoutArea::default();
        }

        let layout = self.layout(inner, config, is_sixel);
        self.render(f, player, colors, tick, config, &layout);
        layout
    }
}

/// Draw the player bar and return the sub-areas it used, so the caller can remember
/// where the progress bar is for mouse hit-testing.
pub(super) fn draw(
    f: &mut Frame,
    player: &PlaybackState,
    tick: u64,
    bs: &BlockStyle<'_>,
    config: &PlayerbarConfig,
    area: Rect,
    is_sixel: bool,
) -> LayoutArea {
    let layout: &dyn Playerbar = match config.layout {
        LayoutType::Default => &default_layout::DefaultLayout,
        LayoutType::Modern => &modern_layout::ModernLayout,
        LayoutType::Minimal => &minimal_layout::MinimalLayout,
    };
    layout.draw(f, player, tick, bs, config, area, is_sixel)
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    use super::*;
    use crate::{
        config::{BorderConfig, LayoutType, PlayerbarConfig, Theme, symbols},
        playback::{BANDS, PlaybackState},
        ui::BlockStyle,
    };

    /// A layout that reserves a row for the spectrum but never draws into it looks exactly
    /// like the feature being switched off — which is how the modern layout shipped once.
    /// Render every layout for real and look for bar glyphs somewhere on the screen.
    #[test]
    fn every_layout_that_reserves_a_row_draws_the_bars() {
        const WIDTH: u16 = 80;
        const HEIGHT: u16 = 5;

        let theme = Theme::default();
        let border = BorderConfig::default();
        let bs = BlockStyle {
            colors: &theme,
            border: &border,
            tick: 0,
        };

        let player = PlaybackState {
            visualizer: vec![1.0; BANDS],
            ..PlaybackState::default()
        };

        let mut config = PlayerbarConfig::default();
        config.visible.visualizer = true;
        config.visible.cover = true;

        let bars: Vec<String> = symbols()
            .visualizer_bars
            .chars()
            .map(|glyph| glyph.to_string())
            .collect();

        for layout in [LayoutType::Default, LayoutType::Modern, LayoutType::Minimal] {
            config.layout = layout.clone();
            let area = Rect::new(0, 0, WIDTH, HEIGHT);

            let mut reserved = Rect::default();
            let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).expect("backend");
            terminal
                .draw(|f| {
                    reserved = draw(f, &player, 0, &bs, &config, area, false).visualizer;
                })
                .expect("draw");

            assert!(
                reserved.width > 0 && reserved.height > 0,
                "{layout:?} reserved no row, so there is nothing for this test to check"
            );

            let buffer = terminal.backend().buffer();
            let drawn = (0..HEIGHT)
                .any(|y| (0..WIDTH).any(|x| bars.iter().any(|bar| buffer[(x, y)].symbol() == bar)));
            assert!(
                drawn,
                "{layout:?} reserved row {reserved:?} but drew no bars anywhere"
            );
        }
    }
}
