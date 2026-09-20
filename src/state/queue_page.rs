//! Geometry the queue page publishes for mouse input.

use ratatui::layout::Rect;

/// Where the queue page drew its tabs and table.
///
/// The page scrolls its own table window (`calc_scroll_offset`) and renders its tabs as
/// one clipped line, so neither position can be derived from a widget state afterwards:
/// both are recorded here while drawing.
#[derive(Debug, Default)]
pub struct QueueHits {
    /// Table body, excluding the scrollbar column.
    pub table: Rect,
    /// First queue index drawn in that table.
    pub offset: usize,
    /// Visible tabs: display key plus the screen area of its label.
    pub tabs: Vec<(String, Rect)>,
}
