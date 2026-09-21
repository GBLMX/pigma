//! Text helpers shared by the clients.

/// A prefix of `text` that is safe to log: at most `max` bytes, cut back to a character
/// boundary.
///
/// The debug logs print a slice of the response body, and that body is remote data —
/// Chinese lyrics, emoji — so slicing at a byte offset panics the moment the offset lands
/// inside a multi-byte character. It only ever hit the log rather than the caller (the
/// response itself parses fine), which is exactly why it stayed unnoticed.
///
/// Takes one byte less, at worst: `max` is a logging budget, not a contract.
pub(crate) fn preview(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }

    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_body_is_returned_whole() {
        assert_eq!(preview("{}", 500), "{}");
        assert_eq!(preview("", 500), "");
    }

    #[test]
    fn a_cut_never_lands_inside_a_character() {
        // Three bytes per character: a cut at 1, 2, 4 or 5 has to move back to 0 or 3.
        for (max, expected) in [
            (1, ""),
            (2, ""),
            (3, "中"),
            (4, "中"),
            (5, "中"),
            (6, "中文"),
        ] {
            assert_eq!(preview("中文", max), expected, "max = {max}");
        }
        // The same for a body that mixes widths, as JSON with Chinese text does.
        let body = "{\"name\":\"晴天\",\"emoji\":\"🌧\"}";
        for max in 0..=body.len() {
            let cut = preview(body, max);
            assert!(body.starts_with(cut), "max = {max}");
            assert!(
                cut.len() <= max,
                "max = {max}: {} bytes is over budget",
                cut.len()
            );
        }
    }
}
