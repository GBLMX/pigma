//! How the lyrics page draws what is being sung.
//!
//! The styles are all reads of the same timed lyric data; they differ in how much of the
//! surrounding song they put on screen and how the current line is coloured.

use serde::{Deserialize, Serialize};

/// Which lyric presentation to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LyricStyle {
    /// A scrolling window of the lines around the current one, with the sung part of the
    /// current line filled by the gradient.
    #[default]
    Window,
    /// One line at a time: only the current line, centred, with the same karaoke fill.
    #[serde(alias = "single")]
    OneLine,
    /// The window again, but the sweep is a single colour (`lyric_ktv_color`) instead of the
    /// gradient: the karaoke-screen look, with the sung part painted over in blue.
    Ktv,
    /// The gradient runs along the text and moves with the music, the neighbouring lines
    /// tinted by the same gradient so the whole page flows.
    Flow,
    /// A plain scrolling list with nothing highlighted.
    Plain,
}

impl LyricStyle {
    /// Every style, in the order the command line offers them.
    pub const ALL: [Self; 5] = [Self::Window, Self::OneLine, Self::Ktv, Self::Flow, Self::Plain];

    /// The name this style is written as in the config and typed in `:lyrics`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::OneLine => "one_line",
            Self::Ktv => "ktv",
            Self::Flow => "flow",
            Self::Plain => "plain",
        }
    }

    /// Parse a style name, accepting the aliases the config accepts.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "window" => Some(Self::Window),
            "one_line" | "single" | "oneline" => Some(Self::OneLine),
            "ktv" | "karaoke" => Some(Self::Ktv),
            "flow" => Some(Self::Flow),
            "plain" => Some(Self::Plain),
            _ => None,
        }
    }

    /// The style after this one, wrapping around: what a bare `:lyrics` cycles through.
    ///
    /// Derived from [`Self::ALL`] instead of a hand-written chain, so adding a style cannot
    /// leave the cycle behind — a new variant used to be skippable here without a word.
    pub fn next(self) -> Self {
        let at = Self::ALL.iter().position(|style| *style == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    /// Short description, for the help and the command line's completion list.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Window => "滚动窗口 + 卡拉OK填充",
            Self::OneLine => "一次只显示当前一行",
            Self::Ktv => "滚动窗口 + 单色（蓝）卡拉OK填充",
            Self::Flow => "渐变沿文字流动，随声音变化",
            Self::Plain => "纯滚动列表，无高亮",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every style has to survive a config round trip, since that is how a user picks one.
    /// Wrapped in a table because that is how the config holds it (and TOML has no bare
    /// values at the top level).
    #[test]
    fn styles_round_trip_through_serde() {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Holder {
            lyric_style: LyricStyle,
        }

        for style in LyricStyle::ALL {
            let text = toml_edit::ser::to_string_pretty(&Holder { lyric_style: style })
                .expect("serialize");
            let back: Holder = toml_edit::de::from_str(&text).expect("deserialize");
            assert_eq!(back.lyric_style, style, "{text}");
            assert_eq!(LyricStyle::parse(style.name()), Some(style));
        }
    }

    /// A bare `:lyrics` walks the styles in the order they are offered and comes back round to
    /// the one it started from — the cycle used to be written out by hand, and a style added to
    /// `ALL` could be missing from it.
    #[test]
    fn the_cycle_visits_every_style_in_order() {
        let mut seen = vec![LyricStyle::Window];
        let mut style = LyricStyle::Window;
        for _ in 1..LyricStyle::ALL.len() {
            style = style.next();
            seen.push(style);
        }

        assert_eq!(seen, LyricStyle::ALL.to_vec());
        assert_eq!(style.next(), LyricStyle::Window);
    }

    /// The names in the config, in the command line and in `name()` are one list.
    #[test]
    fn names_are_unique_and_parse_back() {
        let mut names: Vec<&str> = LyricStyle::ALL.iter().map(|s| s.name()).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "two styles share a name");
        assert_eq!(LyricStyle::parse("single"), Some(LyricStyle::OneLine));
        assert_eq!(LyricStyle::parse(" nope "), None);
    }
}
