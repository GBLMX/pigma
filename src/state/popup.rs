//! A popup's state: whether it is up, where the reader is in it, and how far it goes.
//!
//! Every popup in the app is the same three things — `open`, `scroll`, and the limit the draw pass
//! reports back so the scroll can be clamped at the source instead of running past the bottom —
//! which used to be a struct apiece (`HelpState`, then one per list added after it). One type, so
//! the keys that walk one popup are the keys that walk them all.

/// Whether a popup is up, and where in it the reader is.
#[derive(Debug, Default)]
pub struct PopupState {
    pub open: bool,
    pub scroll: usize,
    /// Scroll limit of the last rendered popup, refreshed on every draw.
    pub max_scroll: usize,
}

impl PopupState {
    pub fn open(&mut self) {
        self.open = true;
        self.scroll = 0;
    }

    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll = (self.scroll + 1).min(self.max_scroll);
    }

    pub fn scroll_top(&mut self) {
        self.scroll = 0;
    }

    pub fn scroll_bottom(&mut self) {
        self.scroll = self.max_scroll;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Opening starts at the top, and the scroll stops where the last draw said the list ends:
    /// without the limit a reader who scrolled past the bottom would have to scroll back through
    /// empty space before the view moved again.
    #[test]
    fn opening_starts_at_the_top_and_scrolling_stops_at_the_bottom() {
        let mut popup = PopupState {
            scroll: 5,
            ..PopupState::default()
        };
        popup.open();
        assert!(popup.open);
        assert_eq!(popup.scroll, 0);

        popup.max_scroll = 3;
        for _ in 0..10 {
            popup.scroll_down();
        }
        assert_eq!(popup.scroll, 3);
        popup.scroll_up();
        assert_eq!(popup.scroll, 2);

        popup.scroll_bottom();
        assert_eq!(popup.scroll, 3);
        popup.scroll_top();
        assert_eq!(popup.scroll, 0);
        popup.scroll_up();
        assert_eq!(popup.scroll, 0, "there is nothing above the first line");

        popup.toggle();
        assert!(!popup.open);
        popup.toggle();
        assert!(popup.open);
    }
}
