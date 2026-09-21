//! How the lyrics page draws what is being sung.
//!
//! The styles are all reads of the same timed lyric data; they differ in how much of the
//! surrounding song they put on screen and how the current line is coloured.

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::utils::{GradientPreset, Named};

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

/// One row of the style table: everything a style decides, in one place.
///
/// The same shape as [`crate::config::ProgressStyle`]'s, and for the same reason: with the name and
/// the description in one row, a new style is a variant plus a row — and everything that reads the
/// table (parsing, the completion list, the toast, the bare `:lyrics` cycle, the help) follows,
/// because there is nowhere else to write it.
struct StyleSpec {
    style: LyricStyle,
    name: &'static str,
    describe: &'static str,
}

impl Named for LyricStyle {
    const ALL: &'static [Self] = &[
        Self::Window,
        Self::OneLine,
        Self::Ktv,
        Self::Flow,
        Self::Plain,
    ];

    /// The spellings that are not the style's own name: the squashed form the first version of
    /// the config accepted, and the word the karaoke style is asked for by.
    const ALIASES: &'static [(&'static str, Self)] = &[
        ("single", Self::OneLine),
        ("oneline", Self::OneLine),
        ("karaoke", Self::Ktv),
    ];

    fn name(self) -> &'static str {
        self.spec().name
    }

    fn describe(self) -> &'static str {
        self.spec().describe
    }
}

impl LyricStyle {
    const SPECS: [StyleSpec; 5] = [
        StyleSpec {
            style: Self::Window,
            name: "window",
            describe: "滚动窗口 + 卡拉OK填充",
        },
        StyleSpec {
            style: Self::OneLine,
            name: "one_line",
            describe: "一次只显示当前一行",
        },
        StyleSpec {
            style: Self::Ktv,
            name: "ktv",
            describe: "滚动窗口 + 单色（蓝）卡拉OK填充",
        },
        StyleSpec {
            style: Self::Flow,
            name: "flow",
            describe: "渐变沿文字流动，随声音变化",
        },
        StyleSpec {
            style: Self::Plain,
            name: "plain",
            describe: "纯滚动列表，无高亮",
        },
    ];

    fn spec(self) -> &'static StyleSpec {
        Self::SPECS
            .iter()
            .find(|spec| spec.style == self)
            .expect("every style has a row")
    }
}

/// What the lyrics page is asked to draw, with everything resolved.
///
/// The page is handed this rather than the whole `Config`: it needs five things, and the colours
/// in it are resolved against the theme in force by [`Config::lyrics_config`] — once a frame
/// rather than once a line, which is also what keeps an unknown colour to one warning per name
/// instead of one per line drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricsConfig<'a> {
    pub style: LyricStyle,
    pub gradient: GradientPreset,
    /// Colour of the sung part in the `ktv` style: a theme field name or a colour of its own,
    /// resolved here.
    pub ktv_color: Color,
    pub show_translation: bool,
    pub title: &'a str,
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

        for style in LyricStyle::ALL.iter().copied() {
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
