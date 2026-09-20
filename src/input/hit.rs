//! Mouse hit-testing helpers.
//!
//! Kept separate from the handlers so the off-by-one risks (a table header row, a scroll
//! offset, a border) are plain arithmetic that can be tested without a terminal.

use ratatui::layout::Rect;

/// Row of a scrolling table under the cursor, as an index into the full list.
///
/// The content table draws its own window (`content::render_content` pre-scrolls the rows
/// it builds) and always renders one header row, so the mapping from a screen row to a
/// list index is `offset + (row − area.y − header_rows)`.
pub(super) fn table_row(
    area: Rect,
    header_rows: u16,
    offset: usize,
    total: usize,
    column: u16,
    row: u16,
) -> Option<usize> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    if column < area.x || column >= area.x + area.width {
        return None;
    }
    if row < area.y || row >= area.y + area.height {
        return None;
    }

    let row_in_window = row.saturating_sub(area.y).saturating_sub(header_rows) as usize;
    if row < area.y + header_rows {
        return None; // the header itself
    }

    let index = offset + row_in_window;
    (index < total).then_some(index)
}

/// Fraction `0.0..=1.0` of a progress bar under the cursor, or `None` outside it.
///
/// Clicks at the very edge land on the first/last cell rather than off the end, which is
/// what a user expects when aiming for the start or the end of a track.
pub(super) fn progress_fraction(area: Rect, column: u16) -> Option<f64> {
    if area.width == 0 || column < area.x || column >= area.x + area.width {
        return None;
    }

    let width = f64::from(area.width);
    let offset = f64::from(column - area.x) + 0.5;
    Some((offset / width).clamp(0.0, 1.0))
}

/// Whether a published area contains the point. A zero-sized area never does, which is
/// what a layout with no room for something publishes.
pub(super) fn contains(area: Rect, column: u16, row: u16) -> bool {
    area.width > 0
        && area.height > 0
        && column >= area.x
        && column < area.x + area.width
        && row >= area.y
        && row < area.y + area.height
}

/// Which navigation item is under the cursor, from the areas published by the last draw.
pub(super) fn nav_item(
    hits: &[(usize, usize, Rect)],
    column: u16,
    row: u16,
) -> Option<(usize, usize)> {
    hits.iter()
        .find(|(_, _, area)| contains(*area, column, row))
        .map(|(section, item, _)| (*section, *item))
}

/// Display key of the queue tab under the cursor.
pub(super) fn queue_tab(tabs: &[(String, Rect)], column: u16, row: u16) -> Option<&str> {
    tabs.iter()
        .find(|(_, area)| contains(*area, column, row))
        .map(|(key, _)| key.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(x: u16, y: u16, width: u16, height: u16) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// The header row must not be clickable, and the row below it is the first item.
    #[test]
    fn table_row_skips_the_header() {
        let area = area(0, 2, 40, 10);
        assert_eq!(table_row(area, 1, 0, 5, 5, 2), None, "header row");
        assert_eq!(table_row(area, 1, 0, 5, 5, 3), Some(0), "first item");
        assert_eq!(table_row(area, 1, 0, 5, 5, 4), Some(1));
    }

    #[test]
    fn table_row_applies_the_scroll_offset() {
        let area = area(0, 0, 40, 5);
        // scrolled down by 100 rows: the first visible row is item 100
        assert_eq!(table_row(area, 1, 100, 500, 1, 1), Some(100));
        assert_eq!(table_row(area, 1, 100, 500, 1, 4), Some(103));
    }

    #[test]
    fn table_row_rejects_clicks_outside_and_past_the_end() {
        let area = area(10, 5, 20, 4);
        assert_eq!(table_row(area, 1, 0, 50, 9, 6), None, "left of the table");
        assert_eq!(table_row(area, 1, 0, 50, 30, 6), None, "right of the table");
        assert_eq!(table_row(area, 1, 0, 50, 15, 4), None, "above the table");
        assert_eq!(table_row(area, 1, 0, 50, 15, 9), None, "below the table");
        // only three items: the last row of the area is past the end
        assert_eq!(table_row(area, 1, 0, 3, 15, 8), Some(2));
        assert_eq!(table_row(area, 1, 0, 3, 15, 9), None);
    }

    /// A zero-height area is what an absent layout publishes, and must never match.
    #[test]
    fn empty_areas_never_match() {
        assert_eq!(table_row(area(0, 0, 0, 0), 1, 0, 10, 0, 0), None);
        assert_eq!(progress_fraction(area(0, 0, 0, 0), 0), None);
    }

    #[test]
    fn progress_fraction_spans_the_bar() {
        let bar = area(10, 20, 40, 1);
        assert_eq!(progress_fraction(bar, 9), None, "left of the bar");
        assert_eq!(progress_fraction(bar, 50), None, "right of the bar");
        assert!((progress_fraction(bar, 10).unwrap() - 0.0125).abs() < 1e-9);
        assert!((progress_fraction(bar, 49).unwrap() - 0.9875).abs() < 1e-9);
        // the middle cell reads as the middle of the track
        let middle = progress_fraction(bar, 29).unwrap();
        assert!((0.4..0.6).contains(&middle), "middle click gave {middle}");
    }

    #[test]
    fn nav_item_matches_the_published_areas() {
        let hits = vec![
            (0, 0, area(0, 1, 20, 1)),
            (0, 1, area(0, 2, 20, 1)),
            (1, 0, area(0, 4, 20, 1)),
        ];
        assert_eq!(nav_item(&hits, 5, 1), Some((0, 0)));
        assert_eq!(nav_item(&hits, 5, 2), Some((0, 1)));
        assert_eq!(nav_item(&hits, 5, 4), Some((1, 0)));
        assert_eq!(
            nav_item(&hits, 5, 3),
            None,
            "a section title is not an item"
        );
        assert_eq!(nav_item(&hits, 25, 1), None, "outside the sidebar");
        assert_eq!(nav_item(&[], 5, 1), None);
    }

    #[test]
    fn queue_tab_matches_the_published_labels() {
        let tabs = vec![
            ("我喜欢的音乐".to_string(), area(2, 1, 12, 1)),
            ("daily".to_string(), area(16, 1, 7, 1)),
        ];
        assert_eq!(queue_tab(&tabs, 3, 1), Some("我喜欢的音乐"));
        assert_eq!(queue_tab(&tabs, 18, 1), Some("daily"));
        assert_eq!(queue_tab(&tabs, 15, 1), None, "the gap between tabs");
        assert_eq!(queue_tab(&tabs, 3, 2), None, "the row below the tabs");
        assert_eq!(queue_tab(&[], 3, 1), None);
    }

    /// `contains` is what every area check goes through, so its edges are the contract:
    /// half-open on both axes, never true for an area with no size.
    #[test]
    fn contains_is_half_open_and_ignores_empty_areas() {
        let rect = area(4, 10, 3, 2);
        assert!(contains(rect, 4, 10));
        assert!(contains(rect, 6, 11));
        assert!(!contains(rect, 7, 11), "right edge is exclusive");
        assert!(!contains(rect, 6, 12), "bottom edge is exclusive");
        assert!(!contains(rect, 3, 10));
        assert!(!contains(area(4, 10, 0, 2), 4, 10));
        assert!(!contains(area(4, 10, 3, 0), 4, 10));
        assert!(!contains(Rect::default(), 0, 0));
    }
}
