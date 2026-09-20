use ratatui::{buffer::Buffer, layout::Rect, style::Style, widgets::Widget};

use crate::config::symbols;

pub struct Spinner {
    tick: u64,
    filled_color: Style,
    empty_color: Style,
}

impl Spinner {
    pub(super) fn new(tick: u64) -> Self {
        Self {
            tick,
            filled_color: Style::default(),
            empty_color: Style::default(),
        }
    }

    pub(super) fn active_color(mut self, style: Style) -> Self {
        self.filled_color = style;
        self
    }

    pub(super) fn inactive_color(mut self, style: Style) -> Self {
        self.empty_color = style;
        self
    }
}

impl Widget for Spinner {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Frames and their speed come from the configured symbol preset; a cell is drawn
        // as filled when it matches the frame's first character, which holds for the
        // block bar and the ascii bar alike.
        let frame = symbols().activity_frame(self.tick);
        let filled = frame.chars().next();

        for (i, ch) in frame.chars().enumerate() {
            let x = area.x + i as u16;
            if x >= area.right() {
                break;
            }
            let style = if Some(ch) == filled {
                self.filled_color
            } else {
                self.empty_color
            };
            buf[(x, area.y)].set_char(ch);
            buf[(x, area.y)].set_style(style);
        }
    }
}
