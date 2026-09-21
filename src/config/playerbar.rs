use serde::{Deserialize, Serialize, Serializer, de};

use crate::utils::{GradientPreset, Named, terminal::ImageProtocolChoice};

/// A named progress-bar look: the symbol pair and the gradient that go together.
///
/// Spelling out "slashes over a rainbow" used to take three keys (`filled_symbol`,
/// `unfilled_symbol`, `gradient_preset`), and the looks worth having are a known few. A style
/// supplies the symbols and the gradient; the two symbol keys stay overridable one at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProgressStyle {
    /// What this bar has always drawn: a thick line, no gradient.
    #[default]
    Thick,
    /// Slanted segments, lit up over a rainbow.
    Segment,
    /// A thin line over a warm gradient.
    Line,
    /// Blocks over `turbo`.
    Blocks,
    /// One glyph, no gradient.
    Plain,
}

/// One row of the style table: everything a style decides, in one place.
///
/// The same shape as `LyricStyle`'s `SPECS`, and for the same reason: with the symbols, the
/// gradient and the name in a single row, a new style is one row plus its variant — nothing
/// else can be forgotten because there is nowhere else to write it.
struct ProgressSpec {
    style: ProgressStyle,
    name: &'static str,
    describe: &'static str,
    filled: &'static str,
    unfilled: &'static str,
    gradient: Option<GradientPreset>,
}

impl ProgressStyle {
    const SPECS: [ProgressSpec; 5] = [
        ProgressSpec {
            style: Self::Thick,
            name: "thick",
            describe: "粗线实心，无渐变",
            filled: "━",
            unfilled: "─",
            gradient: None,
        },
        ProgressSpec {
            style: Self::Segment,
            name: "segment",
            describe: "斜线分段 + 彩虹渐变",
            filled: "/",
            unfilled: "/",
            gradient: Some(GradientPreset::Rainbow),
        },
        ProgressSpec {
            style: Self::Line,
            name: "line",
            describe: "细线 + warm（主题）渐变",
            filled: "─",
            unfilled: "─",
            gradient: Some(GradientPreset::Warm),
        },
        ProgressSpec {
            style: Self::Blocks,
            name: "blocks",
            describe: "方块 + turbo 渐变",
            filled: "█",
            unfilled: "░",
            gradient: Some(GradientPreset::Turbo),
        },
        ProgressSpec {
            style: Self::Plain,
            name: "plain",
            describe: "单字形，无渐变",
            filled: "━",
            unfilled: " ",
            gradient: None,
        },
    ];

    fn spec(self) -> &'static ProgressSpec {
        Self::SPECS
            .iter()
            .find(|spec| spec.style == self)
            .expect("every style has a row")
    }

    pub fn name(self) -> &'static str {
        self.spec().name
    }

    /// Short description, for the toast and the command line's completion list.
    pub fn describe(self) -> &'static str {
        self.spec().describe
    }

    /// The next style, for the bare `:progress`: the cycle is `ALL`'s, so a style cannot be
    /// added and then silently skipped here.
    pub fn next(self) -> Self {
        let at = Self::ALL.iter().position(|style| *style == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    /// The symbols the style draws with: the filled one, and the one the track is made of.
    pub fn symbols(self) -> (&'static str, &'static str) {
        let spec = self.spec();
        (spec.filled, spec.unfilled)
    }

    /// The gradient the style implies; `None` for a flat bar.
    pub fn gradient(self) -> Option<GradientPreset> {
        self.spec().gradient
    }
}
impl Named for ProgressStyle {
    const ALL: &'static [Self] = &[
        Self::Thick,
        Self::Segment,
        Self::Line,
        Self::Blocks,
        Self::Plain,
    ];

    fn name(self) -> &'static str {
        self.spec().name
    }

    fn describe(self) -> &'static str {
        self.spec().describe
    }
}


/// The progress bar's gradient setting, in the three states one key has to express.
///
/// Two states are not enough: the key has to be able to say "no gradient" *and* "whatever the
/// style says". With a plain `Option`, "not written" and "written as nothing" were both `None`,
/// so saving a config whose style supplied the gradient wrote the gradient away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GradientChoice {
    /// Not written: the style decides.
    #[default]
    FollowStyle,
    /// Written as an empty string: no gradient, whatever the style says.
    Off,
    /// A preset, by name.
    Preset(GradientPreset),
}

impl GradientChoice {
    fn is_follow_style(&self) -> bool {
        matches!(self, Self::FollowStyle)
    }
}

impl Serialize for GradientChoice {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::FollowStyle => serializer.serialize_str(""),
            Self::Off => serializer.serialize_str(""),
            Self::Preset(preset) => serializer.serialize_str(preset.name()),
        }
    }
}

impl<'de> Deserialize<'de> for GradientChoice {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        if raw.trim().is_empty() {
            return Ok(Self::Off);
        }
        match GradientPreset::parse(&raw) {
            Some(preset) => Ok(Self::Preset(preset)),
            None => {
                // One line per bad value, not one per frame: the bar is drawn every frame.
                if crate::config::theme::report_unknown_field_once(&format!("gradient {raw}")) {
                    log::warn!("Unknown gradient preset \"{raw}\", drawing a flat bar");
                }
                Ok(Self::Off)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LayoutType {
    #[default]
    Default,
    Modern,
    Minimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerbarVisible {
    pub cover: bool,
    pub volume: bool,
    pub mode_icon: bool,
    pub spinner: bool,
    /// Frequency bars of what is playing, drawn on the layout's spare row.
    pub visualizer: bool,
    /// Dominant-pitch readout (note + frequency), sharing that row with the bars.
    pub pitch: bool,
}

impl Default for PlayerbarVisible {
    fn default() -> Self {
        Self {
            cover: true,
            volume: true,
            mode_icon: true,
            spinner: true,
            visualizer: false,
            pitch: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerbarConfig {
    /// Which look the bar has. The two symbols and the gradient below say "actually, this
    /// instead" one key at a time.
    #[serde(default)]
    pub progress_style: ProgressStyle,
    /// Filled symbol; unset means the style's.
    #[serde(default)]
    pub filled_symbol: Option<String>,
    /// Symbol the track is made of; unset means the style's.
    #[serde(default)]
    pub unfilled_symbol: Option<String>,
    #[serde(default = "default_pb_filled_color")]
    pub filled_color: String,
    #[serde(default = "default_pb_unfilled_color")]
    pub unfilled_color: String,
    #[serde(default = "default_pb_unfilled_color_cached")]
    pub unfilled_color_cached: String,
    /// Progress bar gradient preset. Unset follows the style, an empty string forces the
    /// gradient off, a name forces that preset.
    #[serde(default, skip_serializing_if = "GradientChoice::is_follow_style")]
    pub gradient_preset: GradientChoice,
    /// Turn the cover while a track plays, like a record on a turntable: one turn
    /// in [`crate::playback::CoverState::TURN_SECS`] seconds (20 s), re-encoded at
    /// [`crate::playback::CoverState::STEPS_PER_TURN`] angles a turn (5° apart, one
    /// every 278 ms). Off by default; on a terminal without a graphics protocol the
    /// placeholder glyph and needle turn instead.
    #[serde(default)]
    pub spinning_cover: bool,
    /// How covers are drawn. `auto` asks the terminal and trusts its answer; force
    /// `kitty`, `iterm2`, `sixel` or `halfblocks` where the detection cannot know
    /// better (tmux with `allow-passthrough`, a multiplexer, an ssh hop).
    #[serde(default)]
    pub image_protocol: ImageProtocolChoice,
    #[serde(default)]
    pub layout: LayoutType,
    #[serde(default)]
    pub visible: PlayerbarVisible,
}

fn default_pb_filled_color() -> String {
    "accent".into()
}
fn default_pb_unfilled_color() -> String {
    "text".into()
}
fn default_pb_unfilled_color_cached() -> String {
    "error".into()
}

impl PlayerbarConfig {
    /// The symbol pair the bar draws with: the style's, unless a key overrides it.
    pub fn progress_symbols(&self) -> (&str, &str) {
        let (filled, unfilled) = self.progress_style.symbols();
        (
            self.filled_symbol.as_deref().unwrap_or(filled),
            self.unfilled_symbol.as_deref().unwrap_or(unfilled),
        )
    }

    /// The gradient the bar paints with. This is where the three states of `gradient_preset`
    /// and the style's own gradient collapse into the one answer a gauge needs.
    pub fn progress_gradient(&self) -> Option<GradientPreset> {
        match self.gradient_preset {
            GradientChoice::FollowStyle => self.progress_style.gradient(),
            GradientChoice::Off => None,
            GradientChoice::Preset(preset) => Some(preset),
        }
    }
}

impl Default for PlayerbarConfig {
    fn default() -> Self {
        Self {
            progress_style: ProgressStyle::default(),
            filled_symbol: None,
            unfilled_symbol: None,
            filled_color: default_pb_filled_color(),
            unfilled_color: default_pb_unfilled_color(),
            unfilled_color_cached: default_pb_unfilled_color_cached(),
            gradient_preset: GradientChoice::default(),
            spinning_cover: false,
            image_protocol: ImageProtocolChoice::default(),
            layout: LayoutType::default(),
            visible: PlayerbarVisible::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default has to draw *exactly* what this bar drew before styles existed: the same
    /// thick line, no gradient. A style system that quietly changes the default look is a
    /// regression, not a feature.
    #[test]
    fn the_default_style_draws_what_the_bar_always_drew() {
        let pb = PlayerbarConfig::default();

        assert_eq!(pb.progress_style, ProgressStyle::Thick);
        assert_eq!(pb.progress_symbols(), ("━", "─"));
        assert_eq!(pb.progress_gradient(), None);
    }

    /// A key written by hand still wins over the style — that is what makes a style a starting
    /// point rather than a straitjacket.
    #[test]
    fn a_symbol_written_by_hand_beats_the_style() {
        let pb = PlayerbarConfig {
            progress_style: ProgressStyle::Blocks,
            filled_symbol: Some("=".to_string()),
            ..PlayerbarConfig::default()
        };

        assert_eq!(pb.progress_symbols(), ("=", "░"));
    }

    /// `gradient_preset` has three states and each has to be distinguishable: unset follows the
    /// style, `""` forces the gradient off, a name forces that preset.
    #[test]
    fn the_gradient_key_has_three_states() {
        let style_says = PlayerbarConfig {
            progress_style: ProgressStyle::Blocks,
            ..PlayerbarConfig::default()
        };
        assert_eq!(
            style_says.progress_gradient(),
            ProgressStyle::Blocks.gradient()
        );

        let forced_off = PlayerbarConfig {
            progress_style: ProgressStyle::Blocks,
            gradient_preset: GradientChoice::Off,
            ..PlayerbarConfig::default()
        };
        assert_eq!(forced_off.progress_gradient(), None);

        let forced = PlayerbarConfig {
            progress_style: ProgressStyle::Blocks,
            gradient_preset: GradientChoice::Preset(GradientPreset::Viridis),
            ..PlayerbarConfig::default()
        };
        assert_eq!(forced.progress_gradient(), Some(GradientPreset::Viridis));
    }

    /// Unset has to survive a save. The bar's gradient comes from the style, and a config that
    /// names a style must not come back with its gradient written away — which is what happened
    /// while "not written" and "turned off" were both `None`: saving swallowed the style's
    /// gradient, and every later read saw it as off.
    #[test]
    fn an_unwritten_gradient_is_not_written_by_saving() {
        let written = PlayerbarConfig {
            progress_style: ProgressStyle::Segment,
            ..PlayerbarConfig::default()
        };

        let text = toml_edit::ser::to_string_pretty(&written).expect("serialize");
        let back: PlayerbarConfig = toml_edit::de::from_str(&text).expect("deserialize");

        assert!(!text.contains("gradient_preset"), "{text}");
        assert_eq!(
            back.progress_gradient(),
            Some(GradientPreset::Rainbow),
            "{text}"
        );
    }

    /// An empty string is how a config says "no gradient", and it has to stay that way through a
    /// round trip too.
    #[test]
    fn an_empty_gradient_is_off_and_stays_off() {
        let pb: PlayerbarConfig =
            toml_edit::de::from_str("progress_style = \"segment\"\ngradient_preset = \"\"\n")
                .expect("deserialize");
        assert_eq!(pb.progress_gradient(), None);

        let text = toml_edit::ser::to_string_pretty(&pb).expect("serialize");
        let back: PlayerbarConfig = toml_edit::de::from_str(&text).expect("deserialize");
        assert_eq!(back.progress_gradient(), None, "{text}");
    }

    /// A style has to survive a config round trip, since that is how a user picks one; and the
    /// names in the config, the command line and `name()` are one list.
    #[test]
    fn styles_round_trip_and_their_names_parse_back() {
        // Wrapped in a table, because that is how the config holds it — TOML has no bare
        // values at the top level.
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Holder {
            progress_style: ProgressStyle,
        }

        for style in ProgressStyle::ALL.iter().copied() {
            let text = toml_edit::ser::to_string_pretty(&Holder {
                progress_style: style,
            })
            .expect("serialize");
            let back: Holder = toml_edit::de::from_str(&text).expect("deserialize");

            assert_eq!(back.progress_style, style, "{text}");
            assert_eq!(ProgressStyle::parse(style.name()), Some(style));
        }

        let mut names: Vec<&str> = ProgressStyle::ALL.iter().map(|s| s.name()).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "two styles share a name");
    }

    /// A bare `:progress` walks every style, in the order they are offered, and comes back
    /// round — the cycle is `ALL`'s, so a style cannot be added and then silently skipped.
    #[test]
    fn the_cycle_visits_every_style_in_order() {
        let mut seen = vec![ProgressStyle::Thick];
        let mut style = ProgressStyle::Thick;
        for _ in 1..ProgressStyle::ALL.len() {
            style = style.next();
            seen.push(style);
        }

        assert_eq!(seen, ProgressStyle::ALL.to_vec());
        assert_eq!(style.next(), ProgressStyle::Thick);
    }

    /// The gauge repeats one symbol per cell, so a style whose symbol is empty or two columns
    /// wide would misdraw the bar (and every style has a describe text for the toast).
    #[test]
    fn every_style_has_a_single_cell_symbol_and_a_description() {
        for style in ProgressStyle::ALL.iter().copied() {
            let (filled, unfilled) = style.symbols();

            assert!(!filled.is_empty(), "{}: empty filled symbol", style.name());
            assert!(
                unicode_width::UnicodeWidthStr::width(filled) == 1
                    && unicode_width::UnicodeWidthStr::width(unfilled) == 1,
                "{}: symbols must be one cell wide",
                style.name()
            );
            assert!(!style.describe().is_empty(), "{}", style.name());
        }
    }
}
