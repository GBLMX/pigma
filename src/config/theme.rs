use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::{LazyLock, Mutex, OnceLock},
};

use palette::{LinLuma, Srgb, color_difference::Wcag21RelativeContrast, white_point::D65};
use ratatui::style::{Color, Modifier, Style};
use serde::{Deserialize, Serialize};

use crate::utils::terminal::{
    Background, COLOR_MODE, ColorMode, background_from_luminance, rgb_luminance, rgb_to_16,
    rgb_to_256,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub name: String,
    pub bg: Color,
    pub surface: Color,
    pub text: Color,
    pub accent: Color,
    pub muted: Color,
    pub border: Color,
    pub error: Color,
    /// Amber: a warning is not a failure, and the app had no colour for one until the notices
    /// needed it.
    pub warn: Color,
    /// A section per kind of thing rather than one flat list of hues, so a theme file says "the
    /// selected row" or "the line being sung" instead of guessing which of seven colours a widget
    /// meant — the shape Yazi's theme has, and the reason a flavor can retheme one surface.
    pub table: TableLooks,
    pub tabs: TabLooks,
    pub lyrics: LyricLooks,
    pub popup: PopupLooks,
    pub notify: NotifyLooks,
}

fn cstr(s: &str) -> Color {
    Color::from_str(s).unwrap_or_else(|e| {
        log::warn!("Invalid color '{}' in theme, using fallback: {}", s, e);
        Color::Reset
    })
}

/// Map a true-color value onto what the terminal can display.
///
/// Named colors and palette indices are already terminal-relative and pass through;
/// only `Rgb` needs mapping, so a 256-color terminal stops receiving true-color escapes
/// it cannot render.
fn downsample_color(color: Color, mode: ColorMode) -> Color {
    let Color::Rgb(r, g, b) = color else {
        return color;
    };
    match mode {
        ColorMode::TrueColor => color,
        ColorMode::Ansi256 => Color::Indexed(rgb_to_256(r, g, b)),
        ColorMode::Basic => Color::Indexed(rgb_to_16(r, g, b)),
    }
}

/// A style a theme file writes: colours by name or literal, and the modifiers.
///
/// The shape Yazi and Helix both use — a component's look is `{ fg, bg, bold, italic, … }` rather
/// than one colour — which is what lets a theme say "the selected row is the accent as a
/// background" instead of only which hue to use.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PaintSpec {
    pub fg: Option<ColorSpec>,
    pub bg: Option<ColorSpec>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub reversed: bool,
    pub dim: bool,
}

impl PaintSpec {
    /// The look this adds to `base`: a part left out is the base's.
    fn over(&self, base: Look, theme: &Theme, name: &str) -> Look {
        Look {
            fg: resolve(self.fg.as_ref(), theme, name, "fg").or(base.fg),
            bg: resolve(self.bg.as_ref(), theme, name, "bg").or(base.bg),
            bold: self.bold || base.bold,
            italic: self.italic || base.italic,
            underline: self.underline || base.underline,
            reversed: self.reversed || base.reversed,
            dim: self.dim || base.dim,
        }
    }
}

/// A style the app draws with: the colours resolved, the modifiers decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Look {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub reversed: bool,
    pub dim: bool,
}

impl Look {
    /// The ratatui style for it.
    pub fn style(self) -> Style {
        let mut style = Style::default();
        if let Some(fg) = self.fg {
            style = style.fg(fg);
        }
        if let Some(bg) = self.bg {
            style = style.bg(bg);
        }
        for (on, modifier) in [
            (self.bold, Modifier::BOLD),
            (self.italic, Modifier::ITALIC),
            (self.underline, Modifier::UNDERLINED),
            (self.reversed, Modifier::REVERSED),
            (self.dim, Modifier::DIM),
        ] {
            if on {
                style = style.add_modifier(modifier);
            }
        }

        style
    }
}

/// A colour named after a field of the theme (`accent`, `muted`, …): how a section's defaults are
/// written, so a theme that names only its own colours still has a complete set of looks.
fn named(name: &str) -> Option<ColorSpec> {
    Some(ColorSpec::Text(name.to_string()))
}

/// The list-like surfaces: rows, their header, and the row that is picked out.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TableLooks {
    pub header: PaintSpec,
    pub row: PaintSpec,
    pub selected: PaintSpec,
    pub secondary: PaintSpec,
    pub playing: PaintSpec,
}

impl Default for TableLooks {
    fn default() -> Self {
        Self {
            header: PaintSpec {
                fg: named("accent"),
                bold: true,
                ..PaintSpec::default()
            },
            row: PaintSpec::default(),
            selected: PaintSpec {
                fg: named("on_accent"),
                bg: named("accent"),
                bold: true,
                ..PaintSpec::default()
            },
            secondary: PaintSpec {
                dim: true,
                ..PaintSpec::default()
            },
            playing: PaintSpec {
                fg: named("accent"),
                ..PaintSpec::default()
            },
        }
    }
}

/// The queue's tabs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TabLooks {
    pub active: PaintSpec,
    pub inactive: PaintSpec,
}

impl Default for TabLooks {
    fn default() -> Self {
        Self {
            active: PaintSpec {
                fg: named("on_accent"),
                bg: named("accent"),
                bold: true,
                ..PaintSpec::default()
            },
            inactive: PaintSpec {
                fg: named("muted"),
                ..PaintSpec::default()
            },
        }
    }
}

/// The lyrics page: the line being sung, the ones around it, and the translation under them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LyricLooks {
    pub line: PaintSpec,
    pub sung: PaintSpec,
    pub translation: PaintSpec,
}

impl Default for LyricLooks {
    fn default() -> Self {
        Self {
            line: PaintSpec {
                fg: named("text"),
                ..PaintSpec::default()
            },
            sung: PaintSpec {
                fg: named("accent"),
                bold: true,
                ..PaintSpec::default()
            },
            translation: PaintSpec {
                fg: named("muted"),
                italic: true,
                ..PaintSpec::default()
            },
        }
    }
}

/// A popup: its frame, its title, and the line that says which keys work.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PopupLooks {
    pub border: PaintSpec,
    pub title: PaintSpec,
    pub footer: PaintSpec,
}

impl Default for PopupLooks {
    fn default() -> Self {
        Self {
            border: PaintSpec {
                fg: named("accent"),
                ..PaintSpec::default()
            },
            title: PaintSpec {
                fg: named("accent"),
                bold: true,
                ..PaintSpec::default()
            },
            footer: PaintSpec {
                fg: named("muted"),
                ..PaintSpec::default()
            },
        }
    }
}

/// How loudly a notice says itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotifyLooks {
    pub info: PaintSpec,
    pub warn: PaintSpec,
    pub error: PaintSpec,
}

impl Default for NotifyLooks {
    fn default() -> Self {
        Self {
            info: PaintSpec::default(),
            warn: PaintSpec {
                fg: named("warn"),
                ..PaintSpec::default()
            },
            error: PaintSpec {
                fg: named("error"),
                bold: true,
                ..PaintSpec::default()
            },
        }
    }
}

/// The looks a widget asks for, resolved once when the theme is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Looks {
    pub table_header: Look,
    pub table_row: Look,
    pub table_selected: Look,
    pub table_secondary: Look,
    pub table_playing: Look,
    pub tab_active: Look,
    pub tab_inactive: Look,
    pub lyric_line: Look,
    pub lyric_sung: Look,
    pub lyric_translation: Look,
    pub popup_border: Look,
    pub popup_title: Look,
    pub popup_footer: Look,
    pub notice_info: Look,
    pub notice_warn: Look,
    pub notice_error: Look,
}

impl Theme {
    /// Resolve every section against this theme's own colours: what a spec leaves out comes from
    /// the theme, so a file that sets only `text` and `accent` still has a complete set.
    pub fn looks(&self) -> Looks {
        Looks {
            table_header: self.table.header.over(Look::default(), self, "table.header"),
            table_row: self.table.row.over(
                Look {
                    fg: Some(self.text),
                    ..Look::default()
                },
                self,
                "table.row",
            ),
            table_selected: self.table.selected.over(
                Look {
                    fg: Some(self.on_accent()),
                    bg: Some(self.accent),
                    bold: true,
                    ..Look::default()
                },
                self,
                "table.selected",
            ),
            table_secondary: self.table.secondary.over(
                Look {
                    fg: Some(self.muted),
                    ..Look::default()
                },
                self,
                "table.secondary",
            ),
            table_playing: self.table.playing.over(
                Look {
                    fg: Some(self.accent),
                    bold: true,
                    ..Look::default()
                },
                self,
                "table.playing",
            ),
            tab_active: self.tabs.active.over(
                Look {
                    fg: Some(self.on_accent()),
                    bg: Some(self.accent),
                    bold: true,
                    ..Look::default()
                },
                self,
                "tabs.active",
            ),
            tab_inactive: self.tabs.inactive.over(
                Look {
                    fg: Some(self.muted),
                    ..Look::default()
                },
                self,
                "tabs.inactive",
            ),
            lyric_line: self.lyrics.line.over(
                Look {
                    fg: Some(self.text),
                    ..Look::default()
                },
                self,
                "lyrics.line",
            ),
            lyric_sung: self.lyrics.sung.over(
                Look {
                    fg: Some(self.accent),
                    bold: true,
                    ..Look::default()
                },
                self,
                "lyrics.sung",
            ),
            lyric_translation: self.lyrics.translation.over(
                Look {
                    fg: Some(self.muted),
                    ..Look::default()
                },
                self,
                "lyrics.translation",
            ),
            popup_border: self.popup.border.over(
                Look {
                    fg: Some(self.accent),
                    ..Look::default()
                },
                self,
                "popup.border",
            ),
            popup_title: self.popup.title.over(
                Look {
                    fg: Some(self.accent),
                    bold: true,
                    ..Look::default()
                },
                self,
                "popup.title",
            ),
            popup_footer: self.popup.footer.over(
                Look {
                    fg: Some(self.muted),
                    ..Look::default()
                },
                self,
                "popup.footer",
            ),
            notice_info: self.notify.info.over(
                Look {
                    fg: Some(self.text),
                    ..Look::default()
                },
                self,
                "notify.info",
            ),
            notice_warn: self.notify.warn.over(
                Look {
                    fg: Some(self.warn),
                    ..Look::default()
                },
                self,
                "notify.warn",
            ),
            notice_error: self.notify.error.over(
                Look {
                    fg: Some(self.error),
                    bold: true,
                    ..Look::default()
                },
                self,
                "notify.error",
            ),
        }
    }
}

/// Resolve one colour spec: a field of this theme (`accent`), or a literal the terminal knows.
/// A name this theme does not have is reported once per name, not once per frame.
fn resolve(spec: Option<&ColorSpec>, theme: &Theme, section: &str, part: &str) -> Option<Color> {
    let spec = spec?;
    let ColorSpec::Text(name) = spec else {
        // An index is a colour the terminal already knows.
        return spec.resolve().ok();
    };

    match name.trim() {
        "bg" => return Some(theme.bg),
        "surface" => return Some(theme.surface),
        "text" => return Some(theme.text),
        "accent" => return Some(theme.accent),
        "muted" => return Some(theme.muted),
        "border" => return Some(theme.border),
        "error" => return Some(theme.error),
        "warn" => return Some(theme.warn),
        // The readable foreground on the accent: what the selected row used before it could be
        // themed, and still the right default for a palette the theme did not think about.
        "on_accent" => return Some(theme.on_accent()),
        _ => {}
    }

    match spec.resolve() {
        Ok(color) => Some(color),
        Err(_) => {
            if report_unknown_field_once(&format!("{section}.{part}={name}")) {
                log::warn!("[{section}] {part}: \"{name}\" is not a colour; ignored");
            }
            Some(Color::Reset)
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            bg: cstr("#0e0e0e"),
            surface: cstr("#160e12"),
            text: cstr("#ffffff"),
            accent: cstr("#c20c0c"),
            muted: cstr("#555555"),
            border: Color::Reset,
            error: cstr("#f4535a"),
            warn: cstr("#e5c07b"),
            table: TableLooks::default(),
            tabs: TabLooks::default(),
            lyrics: LyricLooks::default(),
            popup: PopupLooks::default(),
            notify: NotifyLooks::default(),
        }
    }
}

impl Theme {
    fn terminal() -> Self {
        Self {
            name: "terminal".to_string(),
            bg: Color::Indexed(0),
            text: Color::Indexed(15),
            accent: Color::Indexed(5),
            muted: Color::Indexed(4),
            border: Color::Indexed(1),
            error: Color::Indexed(9),
            surface: Color::Indexed(1),
            ..Self::default()
        }
    }

    fn dracula() -> Self {
        Self {
            name: "dracula".to_string(),
            bg: cstr("#282a36"),
            surface: cstr("#44475a"),
            text: cstr("#f8f8f2"),
            accent: cstr("#bd93f9"),
            muted: cstr("#6272a4"),
            border: cstr("#6272a4"),
            error: cstr("#ff5555"),
            ..Self::default()
        }
    }

    fn nord() -> Self {
        Self {
            name: "nord".to_string(),
            bg: cstr("#2e3440"),
            surface: cstr("#3b4252"),
            text: cstr("#eceff4"),
            accent: cstr("#88c0d0"),
            muted: cstr("#616e88"),
            border: cstr("#616e88"),
            error: cstr("#bf616a"),
            ..Self::default()
        }
    }

    fn gruvbox_dark() -> Self {
        Self {
            name: "gruvbox".to_string(),
            bg: cstr("#282828"),
            surface: cstr("#3c3836"),
            text: cstr("#ebdbb2"),
            accent: cstr("#d65d0e"),
            muted: cstr("#928374"),
            border: cstr("#928374"),
            error: cstr("#fb4934"),
            ..Self::default()
        }
    }

    fn solarized_dark() -> Self {
        Self {
            name: "solarized".to_string(),
            bg: cstr("#002b36"),
            surface: cstr("#073642"),
            text: cstr("#839496"),
            accent: cstr("#268bd2"),
            muted: cstr("#586e75"),
            border: cstr("#586e75"),
            error: cstr("#dc322f"),
            ..Self::default()
        }
    }

    fn tokyo_night() -> Self {
        Self {
            name: "tokyo-night".to_string(),
            bg: cstr("#1a1b26"),
            surface: cstr("#24283b"),
            text: cstr("#c0caf5"),
            accent: cstr("#7aa2f7"),
            muted: cstr("#565f89"),
            border: cstr("#565f89"),
            error: cstr("#f7768e"),
            ..Self::default()
        }
    }

    fn catppuccin_mocha() -> Self {
        Self {
            name: "catppuccin".to_string(),
            bg: cstr("#1e1e2e"),
            surface: cstr("#313244"),
            text: cstr("#cdd6f4"),
            accent: cstr("#cba6f7"),
            muted: cstr("#6c7086"),
            border: cstr("#6c7086"),
            error: cstr("#f38ba8"),
            ..Self::default()
        }
    }

    fn one_dark() -> Self {
        Self {
            name: "one-dark".to_string(),
            bg: cstr("#282c34"),
            surface: cstr("#3e4451"),
            text: cstr("#abb2bf"),
            accent: cstr("#61afef"),
            muted: cstr("#5c6370"),
            border: cstr("#5c6370"),
            error: cstr("#e06c75"),
            ..Self::default()
        }
    }

    fn monokai() -> Self {
        Self {
            name: "monokai".to_string(),
            bg: cstr("#272822"),
            surface: cstr("#3e3d32"),
            text: cstr("#f8f8f2"),
            accent: cstr("#f92672"),
            muted: cstr("#75715e"),
            border: cstr("#75715e"),
            error: cstr("#f92672"),
            ..Self::default()
        }
    }

    fn rose_pine() -> Self {
        Self {
            name: "rose-pine".to_string(),
            bg: cstr("#191724"),
            surface: cstr("#26233a"),
            text: cstr("#e0def4"),
            accent: cstr("#eb6f92"),
            muted: cstr("#6e6a86"),
            border: cstr("#6e6a86"),
            error: cstr("#eb6f92"),
            ..Self::default()
        }
    }

    fn kanagawa() -> Self {
        Self {
            name: "kanagawa".to_string(),
            bg: cstr("#1f1f28"),
            surface: cstr("#2a2a37"),
            text: cstr("#dcd7ba"),
            accent: cstr("#7e9cd8"),
            muted: cstr("#727169"),
            border: cstr("#727169"),
            error: cstr("#c34043"),
            ..Self::default()
        }
    }

    //light theme
    fn solarized_light() -> Self {
        Self {
            name: "solarized-light".to_string(),
            bg: cstr("#fdf6e3"),
            surface: cstr("#eee8d5"),
            text: cstr("#5d8796"),
            accent: cstr("#268bd2"),
            muted: cstr("#93a1a1"),
            border: cstr("#93a1a1"),
            error: cstr("#dc322f"),
            ..Self::default()
        }
    }

    fn catppuccin_latte() -> Self {
        Self {
            name: "catppuccin-latte".to_string(),
            bg: cstr("#eff1f5"),
            surface: cstr("#e6e9ef"),
            text: cstr("#4c4f69"),
            accent: cstr("#7287fd"),
            muted: cstr("#9ca0b0"),
            border: cstr("#9ca0b0"),
            error: cstr("#d20f39"),
            ..Self::default()
        }
    }

    fn one_light() -> Self {
        Self {
            name: "one-light".to_string(),
            bg: cstr("#fafafa"),
            surface: cstr("#f0f0f0"),
            text: cstr("#383a42"),
            accent: cstr("#4078f2"),
            muted: cstr("#a0a1a7"),
            border: cstr("#a0a1a7"),
            error: cstr("#e45649"),
            ..Self::default()
        }
    }

    fn github_light() -> Self {
        Self {
            name: "github-light".to_string(),
            bg: cstr("#ffffff"),
            surface: cstr("#f6f8fa"),
            text: cstr("#24292f"),
            accent: cstr("#0969da"),
            muted: cstr("#57606a"),
            border: cstr("#57606a"),
            error: cstr("#cf222e"),
            ..Self::default()
        }
    }

    fn gruvbox_light() -> Self {
        Self {
            name: "gruvbox-light".to_string(),
            bg: cstr("#fbf1c7"),
            surface: cstr("#ebdbb2"),
            text: cstr("#3c3836"),
            accent: cstr("#d65d0e"),
            muted: cstr("#928374"),
            border: cstr("#928374"),
            error: cstr("#cc241d"),
            ..Self::default()
        }
    }

    fn cyberpunk_hot() -> Self {
        Self {
            name: "cyberpunk-hot".to_string(),
            bg: cstr("#0a0b10"),
            surface: cstr("#1a1a2e"),
            text: cstr("#ff0066"),
            accent: cstr("#ffcc00"),
            muted: cstr("#00f0ff"),
            border: Color::Reset,
            error: cstr("#b000ff"),
            ..Self::default()
        }
    }

    fn cyberpunk_fury() -> Self {
        Self {
            name: "cyberpunk-fury".to_string(),
            bg: cstr("#0d0a14"),
            surface: cstr("#2a1a3a"),
            text: cstr("#ffdd00"),
            accent: cstr("#ff00aa"),
            muted: cstr("#00ccff"),
            border: Color::Reset,
            error: cstr("#ff3300"),
            ..Self::default()
        }
    }

    fn cyberpunk_volt() -> Self {
        Self {
            name: "cyberpunk-volt".to_string(),
            bg: cstr("#080c14"),
            surface: cstr("#111827"),
            text: cstr("#00f0ff"),
            accent: cstr("#ff0066"),
            muted: cstr("#ccff00"),
            border: Color::Reset,
            error: cstr("#00ff66"),
            ..Self::default()
        }
    }

    /// Convert every true-color token to the terminal's palette.
    ///
    /// The theme's own colour that stays readable when drawn on top of [`Self::accent`].
    ///
    /// The selected table row is painted with `accent` as its background, but its cells
    /// keep whatever colour their column asked for — and dimmed columns are [`Self::muted`],
    /// which on `accent` is nearly invisible in the light palettes (github-light measures
    /// 1.2:1, gruvbox-light 1.05:1). Whichever of the palette's two extremes is further
    /// from `accent` keeps this true for user-written themes as well, instead of assuming
    /// `bg` is the right one.
    pub fn on_accent(&self) -> Color {
        let Some(accent) = relative_luminance(self.accent) else {
            return self.bg;
        };
        let black = LinLuma::<D65, f64>::new(0.0);
        let white = LinLuma::<D65, f64>::new(1.0);
        let candidates = [
            relative_luminance(self.bg).map(|l| (contrast_ratio(l, accent), self.bg)),
            relative_luminance(self.text).map(|l| (contrast_ratio(l, accent), self.text)),
            // Palettes where both anchors sit close to the accent (catppuccin-latte and
            // one-light among the built-ins) cannot reach 3:1 with their own colours, and a
            // row nobody can read is worse than a plain black-or-white highlight.
            Some((contrast_ratio(black, accent), Color::Rgb(0, 0, 0))),
            Some((contrast_ratio(white, accent), Color::Rgb(255, 255, 255))),
        ];

        candidates
            .into_iter()
            .flatten()
            .filter(|(ratio, _)| *ratio >= 3.0)
            .max_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(_, color)| color)
            .unwrap_or(self.bg)
    }

    /// Applied once when a theme is loaded, so the render path keeps working with plain
    /// ratatui colors and nothing has to know about the terminal's capabilities.
    pub fn downsampled(self, mode: ColorMode) -> Self {
        if mode == ColorMode::TrueColor {
            return self;
        }
        Self {
            name: self.name,
            bg: downsample_color(self.bg, mode),
            surface: downsample_color(self.surface, mode),
            text: downsample_color(self.text, mode),
            accent: downsample_color(self.accent, mode),
            muted: downsample_color(self.muted, mode),
            border: downsample_color(self.border, mode),
            warn: downsample_color(self.warn, mode),
            table: self.table,
            tabs: self.tabs,
            lyrics: self.lyrics,
            popup: self.popup,
            notify: self.notify,
            error: downsample_color(self.error, mode),
        }
    }

    /// Whether this theme's own background is light.
    ///
    /// The slot the theme came from is the config's business; this is what its colours actually
    /// say, which is what decides whether the page needs its background painted over the
    /// terminal's (see [`BackgroundFill`](crate::utils::terminal::BackgroundFill)). The threshold
    /// is the terminal probe's, so the two answers are comparable.
    pub fn background(&self) -> Background {
        match self.bg {
            Color::Rgb(r, g, b) => background_from_luminance(rgb_luminance(
                f64::from(r) / 255.0,
                f64::from(g) / 255.0,
                f64::from(b) / 255.0,
            )),
            // A palette colour or `Reset` says nothing measurable about the screen it lands on;
            // the probe's own fallback is the same assumption.
            _ => Background::Dark,
        }
    }

    /// Resolve a colour written in the config: a theme field, or a colour of its own.
    ///
    /// [`Self::field_color`] is what the playerbar's colours use, because a progress bar should
    /// follow the theme. The lyrics' `ktv` fill is the opposite case — karaoke screens paint
    /// blue and the requested default is blue — so this accepts either spelling: a field name is
    /// read from the theme, anything else goes through `Color::from_str` (a name like `blue` or
    /// `lightblue`, `#rrggbb`, an ANSI index). An unreadable name falls back to the accent and is
    /// reported once, the same way an unknown field name is.
    pub fn resolve_color(&self, spec: &str) -> Color {
        match spec {
            "bg" | "surface" | "text" | "accent" | "muted" | "border" | "error" => {
                self.field_color(spec)
            }
            spec => Color::from_str(spec).unwrap_or_else(|_| {
                if report_unknown_field_once(spec) {
                    log::warn!("Unknown colour: \"{spec}\", falling back to accent");
                }
                self.accent
            }),
        }
    }

    /// Look up a theme color field by name (e.g. "accent", "muted", "error", "border").
    pub fn field_color(&self, name: &str) -> Color {
        match name {
            "bg" => self.bg,
            "surface" => self.surface,
            "text" => self.text,
            "accent" => self.accent,
            "muted" => self.muted,
            "border" => self.border,
            "error" => self.error,
            _ => {
                if report_unknown_field_once(name) {
                    log::warn!("Unknown theme field: \"{name}\", falling back to accent");
                }
                self.accent
            }
        }
    }
}

/// Whether this unknown field name still needs reporting.
///
/// [`Theme::field_color`] is called from the render path with names taken out of the user's
/// config, so an unknown name used to write one warning **per frame** — the user's
/// `unfilled_color_cached = "warning"` produced five log lines a second, and the log file grew
/// without bound again. The lookup has to stay on the render path (that is how config-driven
/// colours work), but the warning only has to be said once per name per run.
pub(crate) fn report_unknown_field_once(name: &str) -> bool {
    static REPORTED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let reported = REPORTED.get_or_init(Default::default);
    match reported.lock() {
        Ok(mut reported) => reported.insert(name.to_string()),
        // A poisoned lock means some other thread panicked while reporting; stay quiet rather
        // than panicking here as well.
        Err(_) => false,
    }
}

/// WCAG relative luminance, for colours that carry their own channels. Palette indices and
/// named colours depend on the terminal's own palette, which the app cannot read.
///
/// The number itself comes from `palette`: sRGB decoding plus the Y row of the sRGB→XYZ
/// matrix, which is what WCAG 2.1 defines as relative luminance.
pub(crate) fn relative_luminance(color: Color) -> Option<LinLuma<D65, f64>> {
    let Color::Rgb(r, g, b) = color else {
        return None;
    };
    Some(Srgb::new(r, g, b).into_format::<f64>().relative_luminance())
}

/// WCAG contrast ratio between two relative luminances.
pub(crate) fn contrast_ratio(a: LinLuma<D65, f64>, b: LinLuma<D65, f64>) -> f64 {
    a.relative_contrast(b)
}

/// Built-in themes, resolved once and down-sampled to the terminal's palette.
fn builtin_themes() -> &'static HashMap<String, Theme> {
    static THEMES: LazyLock<HashMap<String, Theme>> = LazyLock::new(|| {
        let mode = *COLOR_MODE;
        let themes: Vec<Theme> = vec![
            Theme::default(),
            Theme::terminal(),
            Theme::dracula(),
            Theme::nord(),
            Theme::gruvbox_dark(),
            Theme::solarized_dark(),
            Theme::tokyo_night(),
            Theme::catppuccin_mocha(),
            Theme::one_dark(),
            Theme::monokai(),
            Theme::rose_pine(),
            Theme::kanagawa(),
            Theme::github_light(),
            Theme::gruvbox_light(),
            Theme::one_light(),
            Theme::catppuccin_latte(),
            Theme::solarized_light(),
            Theme::cyberpunk_volt(),
            Theme::cyberpunk_fury(),
            Theme::cyberpunk_hot(),
        ];
        themes
            .into_iter()
            .map(|t| {
                let t = t.downsampled(mode);
                let n = t.name.clone();
                (n, t)
            })
            .collect()
    });
    &THEMES
}

/// A colour as the config writes it: `"#rrggbb"`, a name like `"red"`, an index like
/// `"3"`, or just the integer `3`. That is the format `config.example.toml` documents, and
/// the one the built-in themes are written in.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ColorSpec {
    Index(u8),
    Text(String),
}

impl ColorSpec {
    fn resolve(&self) -> Result<Color, String> {
        match self {
            ColorSpec::Index(index) => Ok(Color::Indexed(*index)),
            ColorSpec::Text(text) => {
                let text = text.trim();
                if let Ok(index) = text.parse::<u8>() {
                    return Ok(Color::Indexed(index));
                }
                Color::from_str(text).map_err(|error| error.to_string())
            }
        }
    }
}

/// A theme as the config writes it: `[themes.<name>]` starts from `base` — a built-in, or
/// another user theme, `default` when unset — and overrides whichever colours it lists.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct UserTheme {
    pub base: Option<String>,
    pub bg: Option<ColorSpec>,
    pub surface: Option<ColorSpec>,
    pub text: Option<ColorSpec>,
    pub accent: Option<ColorSpec>,
    pub muted: Option<ColorSpec>,
    pub border: Option<ColorSpec>,
    pub error: Option<ColorSpec>,
    pub warn: Option<ColorSpec>,
    /// The component sections: a theme file says "the selected row" or "the line being sung"
    /// rather than only which of the flat colours a widget meant.
    pub table: Option<TableLooks>,
    pub tabs: Option<TabLooks>,
    pub lyrics: Option<LyricLooks>,
    pub popup: Option<PopupLooks>,
    pub notify: Option<NotifyLooks>,
}

impl UserTheme {
    /// The concrete theme: the base with whatever this theme parses. A colour that cannot
    /// be parsed costs that one colour rather than the whole config.
    fn resolve(&self, name: &str, base: &Theme) -> Theme {
        let mut theme = base.clone();
        theme.name = name.to_string();
        set(&mut theme.bg, self.bg.as_ref(), name, "bg");
        set(&mut theme.surface, self.surface.as_ref(), name, "surface");
        set(&mut theme.text, self.text.as_ref(), name, "text");
        set(&mut theme.accent, self.accent.as_ref(), name, "accent");
        set(&mut theme.muted, self.muted.as_ref(), name, "muted");
        set(&mut theme.border, self.border.as_ref(), name, "border");
        set(&mut theme.error, self.error.as_ref(), name, "error");
        set(&mut theme.warn, self.warn.as_ref(), name, "warn");

        // A section a theme writes replaces the base's: what it leaves out of that section is
        // filled from the theme's own colours when the looks are resolved (see `Theme::looks`), so
        // `[table] selected = …` changes that one look and nothing else.
        if let Some(table) = &self.table {
            theme.table = table.clone();
        }
        if let Some(tabs) = &self.tabs {
            theme.tabs = tabs.clone();
        }
        if let Some(lyrics) = &self.lyrics {
            theme.lyrics = lyrics.clone();
        }
        if let Some(popup) = &self.popup {
            theme.popup = popup.clone();
        }
        if let Some(notify) = &self.notify {
            theme.notify = notify.clone();
        }

        theme
    }
}

fn set(target: &mut Color, value: Option<&ColorSpec>, name: &str, field: &str) {
    let Some(value) = value else {
        return;
    };
    match value.resolve() {
        Ok(color) => *target = color,
        Err(error) => {
            log::warn!("自定义主题 `{name}` 的 {field} 无法解析（{error}），沿用 base 的颜色")
        }
    }
}

pub struct ThemeRegistry {
    extras: HashMap<String, Theme>,
}

/// The name a user may write where a theme name goes: pick one at random.
pub const RANDOM_THEME: &str = "random";

/// The built-in a user theme starts from when it names no base.
const DEFAULT_BASE: &str = "default";

impl ThemeRegistry {
    /// Built-ins plus the user's themes, which may build on a built-in or on another user
    /// theme and override individual colours.
    pub fn new(user: HashMap<String, UserTheme>) -> Self {
        let mode = *COLOR_MODE;
        let builtins = builtin_themes();
        let mut extras: HashMap<String, Theme> = HashMap::new();
        let mut pending: Vec<(String, UserTheme)> = user.into_iter().collect();

        // Resolve in passes so a theme may start from one that was resolved before it; a
        // base that never appears is a typo or a cycle, and is reported rather than looping.
        while !pending.is_empty() {
            let mut resolved: Vec<(String, Theme)> = Vec::new();
            for (name, spec) in &pending {
                let base = match spec.base.as_deref() {
                    None => builtins.get(DEFAULT_BASE),
                    Some(base) => extras.get(base).or_else(|| builtins.get(base)),
                };
                if let Some(base) = base {
                    resolved.push((name.clone(), spec.resolve(name, base).downsampled(mode)));
                }
            }
            if resolved.is_empty() {
                for (name, spec) in &pending {
                    log::warn!(
                        "自定义主题 `{name}` 的 base `{}` 不存在或成环，已忽略",
                        spec.base.as_deref().unwrap_or(DEFAULT_BASE)
                    );
                }
                break;
            }
            pending.retain(|(name, _)| !resolved.iter().any(|(done, _)| done == name));
            extras.extend(resolved);
        }

        Self { extras }
    }

    pub fn get(&self, name: &str) -> Option<&Theme> {
        self.extras.get(name).or_else(|| builtin_themes().get(name))
    }

    /// The themes this registry holds, in a stable order: the map's own order varies per
    /// process, which would make `:theme` cycle differently on every start.
    pub fn all_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.extras.keys().map(|s| s.as_str()).collect();
        for k in builtin_themes().keys() {
            if !self.extras.contains_key(k) {
                names.push(k);
            }
        }
        names.sort_unstable();
        names
    }

    /// What may be written where a theme name goes: [`RANDOM_THEME`], then every theme.
    ///
    /// `random` comes first on purpose. Cycling walks this list and then continues from
    /// whichever theme was picked, so putting it first means it comes up once per lap instead
    /// of swallowing the step after it.
    pub fn choosable_names(&self) -> Vec<&str> {
        let mut names = vec![RANDOM_THEME];
        names.extend(self.all_names());
        names
    }

    /// The theme [`RANDOM_THEME`] stands for, drawn uniformly from [`Self::all_names`].
    pub fn random_name(&self) -> &str {
        let names = self.all_names();
        let count = names.len();
        if count == 0 {
            return theme_fallback().name.as_str();
        }
        names[rand::random_range(0..count)]
    }

    /// The theme behind `requested`: [`RANDOM_THEME`] becomes one of the themes, and every
    /// other name — including a typo — is passed through untouched.
    ///
    /// Call this where a theme is *chosen*, never where it is *drawn*: `resolve_theme` runs on
    /// every frame, and rolling there would repaint the interface in a new theme each frame.
    pub fn concrete_name(&self, requested: &str) -> String {
        if requested == RANDOM_THEME {
            self.random_name().to_string()
        } else {
            requested.to_string()
        }
    }
}

/// Last-resort hardcoded theme used when neither the configured theme nor the
/// built-in `default` theme is available.
pub fn theme_fallback() -> &'static Theme {
    static FALLBACK: LazyLock<Theme> = LazyLock::new(|| Theme::default().downsampled(*COLOR_MODE));
    &FALLBACK
}

#[cfg(test)]
mod random_theme_tests {
    use super::*;

    fn registry() -> ThemeRegistry {
        ThemeRegistry::new(HashMap::new())
    }

    /// Whatever `random` picks has to be a theme that resolves, or `resolve_theme` logs a
    /// warning and the user sees `default` while their config says `random`.
    #[test]
    fn random_always_picks_a_theme_that_exists() {
        let registry = registry();
        for _ in 0..64 {
            let picked = registry.random_name();
            assert_ne!(picked, RANDOM_THEME, "`random` is a request, not a theme");
            assert!(
                registry.get(picked).is_some(),
                "`{picked}` does not resolve"
            );
        }
    }

    #[test]
    fn concrete_name_passes_every_other_name_through() {
        let registry = registry();
        assert_eq!(registry.concrete_name("dracula"), "dracula");
        assert_eq!(
            registry.concrete_name("a-typo-stays-a-typo"),
            "a-typo-stays-a-typo"
        );

        let rolled = registry.concrete_name(RANDOM_THEME);
        assert_ne!(rolled, RANDOM_THEME);
        assert!(
            registry.get(&rolled).is_some(),
            "`{rolled}` does not resolve"
        );
    }

    /// `random` heads the list so that cycling meets it once per lap: the theme it picks is
    /// further down the list, so the next step carries on past the entry instead of landing on
    /// `random` again.
    #[test]
    fn choosable_names_lists_random_then_every_theme() {
        let registry = registry();
        let choosable = registry.choosable_names();
        assert_eq!(choosable.first().copied(), Some(RANDOM_THEME));
        assert_eq!(choosable.len(), registry.all_names().len() + 1);
        assert_eq!(&choosable[1..], registry.all_names().as_slice());
    }

    /// The list is a map's keys, so without sorting the cycle order would differ per run.
    #[test]
    fn all_names_is_stable() {
        let registry = registry();
        let names = registry.all_names();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
    }
}

#[cfg(test)]
mod user_theme_tests {
    use super::*;

    /// Go through the real loader: a config file carrying a `[themes.<name>]` table has to
    /// load at all, which is what used to fail for the documented hex colours.
    fn user(label: &str, source: &str) -> HashMap<String, UserTheme> {
        let dir = std::env::temp_dir().join(format!(
            "boxpigma-theme-test-{}-{label}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        let path = dir.join("config.toml");
        std::fs::write(&path, source).expect("write config");

        let themes = crate::config::Config::load_from(&path).themes;
        let _ = std::fs::remove_dir_all(&dir);
        themes
    }

    fn expected(color: &str) -> Color {
        downsample_color(cstr(color), *COLOR_MODE)
    }

    /// The selected table row is painted on `accent`, so the colour the theme picks for it
    /// has to be readable there — this is what made the row vanish in the light palettes.
    #[test]
    fn on_accent_is_readable_for_every_builtin_theme() {
        for name in builtin_themes().keys() {
            let theme = builtin_themes().get(name).expect("theme");
            let (Some(fg), Some(bg)) = (
                relative_luminance(theme.on_accent()),
                relative_luminance(theme.accent),
            ) else {
                continue; // palette-index themes: `on_accent` falls back to `bg` by design
            };
            let ratio = contrast_ratio(fg, bg);
            // 3:1 is the WCAG bar for large text and UI parts, which is what a highlighted
            // row is. That is the contract: whatever the palette looks like, the row drawn
            // on `accent` stays readable.
            assert!(
                ratio >= 3.0,
                "{name}: text on accent is {ratio:.2}:1, below the 3:1 readable minimum"
            );
        }
    }

    /// A palette that cannot be measured (named or indexed colours) keeps the old answer.
    #[test]
    fn on_accent_falls_back_for_palette_colours() {
        let mut theme = Theme::terminal();
        theme.bg = Color::Indexed(0);
        assert_eq!(theme.on_accent(), theme.bg);
    }

    fn builtin(name: &str) -> &'static Theme {
        builtin_themes().get(name).expect("built in")
    }

    /// The formats `config.example.toml` documents — hex, a name, an index — are the ones
    /// users write, and none of them used to survive deserialization.
    #[test]
    fn user_themes_start_from_a_base_and_override_what_they_list() {
        let registry = ThemeRegistry::new(user(
            "base",
            r##"
[themes.mine]
base = "dracula"
bg = "#0a0b10"
accent = "red"
border = 3
"##,
        ));

        let theme = registry.get("mine").expect("registered");
        assert_eq!(theme.name, "mine");
        assert_eq!(theme.bg, expected("#0a0b10"));
        assert_eq!(theme.accent, expected("red"));
        assert_eq!(theme.border, Color::Indexed(3));
        assert_eq!(
            theme.text,
            builtin("dracula").text,
            "unlisted colours come from the base"
        );
        assert!(
            registry.all_names().contains(&"mine"),
            "and it can be selected"
        );
    }

    /// No base means `default`, so a one-line theme is enough.
    #[test]
    fn a_user_theme_without_a_base_starts_from_default() {
        let registry = ThemeRegistry::new(user("tiny", "[themes.tiny]\naccent = \"#ff0000\"\n"));
        let theme = registry.get("tiny").expect("registered");
        assert_eq!(theme.accent, expected("#ff0000"));
        assert_eq!(theme.text, builtin("default").text);
    }

    /// One bad colour must not cost the rest of the theme, and a bad base must not be
    /// registered at all — neither may take the config down with them.
    #[test]
    fn bad_values_are_contained() {
        let registry = ThemeRegistry::new(user(
            "bad",
            r##"
[themes.typo]
base = "default"
bg = "#nothex"

[themes.orphan]
base = "nope"
accent = "#ffffff"
"##,
        ));

        let typo = registry.get("typo").expect("registered");
        assert_eq!(
            typo.bg,
            builtin("default").bg,
            "the base value survives the typo"
        );
        assert!(
            registry.get("orphan").is_none(),
            "an unknown base is ignored"
        );
    }
}

#[cfg(test)]
mod unknown_field_tests {
    use super::*;

    #[test]
    fn an_unknown_field_falls_back_to_accent_and_is_reported_once() {
        let theme = Theme::default();
        let name = "__definitely_not_a_theme_field__";
        assert_eq!(theme.field_color(name), theme.accent, "兜底应是 accent");
        assert_eq!(theme.field_color(name), theme.accent, "第二次仍应兜底");
        assert!(!report_unknown_field_once(name), "同一个名字第二次不应再报");
        assert!(
            report_unknown_field_once("__another_unknown_field__"),
            "不同的名字仍应各报一次"
        );
    }
}

#[cfg(test)]
mod look_tests {
    use super::*;

    /// With nothing written in a section, the looks are the theme's own colours: a theme that sets
    /// only `text` and `accent` still has a complete set, which is what makes the sections
    /// additive rather than something every theme has to write out in full.
    #[test]
    fn the_defaults_come_from_the_themes_own_colours() {
        let theme = Theme::default();
        let looks = theme.looks();

        assert_eq!(looks.table_row.fg, Some(theme.text));
        assert_eq!(looks.table_header.fg, Some(theme.accent));
        assert_eq!(looks.table_selected.bg, Some(theme.accent));
        assert_eq!(looks.table_secondary.fg, Some(theme.muted));
        assert_eq!(looks.tab_active.bg, Some(theme.accent));
        assert_eq!(looks.lyric_line.fg, Some(theme.text));
        assert_eq!(looks.notice_error.fg, Some(theme.error));
        assert_eq!(looks.notice_warn.fg, Some(theme.warn));
        assert!(looks.notice_error.bold);
    }

    /// A section in a theme file is what a widget draws with: the part it names is taken, and the
    /// parts it leaves out still come from the theme's colours — which is why a theme can write
    /// one line about one row and nothing else.
    #[test]
    fn a_section_in_a_theme_file_is_what_a_widget_draws_with() {
        let user: UserTheme = toml_edit::de::from_str(
            r##"
base = "terminal"
[table]
selected = { bg = "#123456", italic = true }
[tabs]
active = { fg = "accent" }
"##,
        )
        .expect("a theme file");

        let base = Theme::terminal();
        let theme = user.resolve("mine", &base);
        let looks = theme.looks();

        assert_eq!(looks.table_selected.bg, Some(cstr("#123456")));
        assert!(looks.table_selected.italic, "the modifier is the theme's");
        assert_eq!(
            looks.table_selected.fg,
            Some(theme.on_accent()),
            "what the section left out still comes from the theme"
        );
        assert_eq!(
            looks.table_row.fg,
            Some(theme.text),
            "and the rest is untouched"
        );
        assert_eq!(looks.tab_active.fg, Some(theme.accent));
        assert_eq!(
            looks.table_header.fg,
            Some(theme.accent),
            "a section the theme did not write keeps its own defaults"
        );
    }

    /// A colour name the theme does not have is not a reason to lose the row: it falls back, the
    /// way every other unknown colour in the config does, and says so once.
    #[test]
    fn a_colour_name_the_theme_does_not_have_falls_back() {
        let user: UserTheme = toml_edit::de::from_str(
            r##"
[table]
row = { fg = "not_a_colour_name" }
"##,
        )
        .expect("a theme file");

        let theme = user.resolve("mine", &Theme::default());

        assert_eq!(theme.looks().table_row.fg, Some(Color::Reset));
    }
}

#[cfg(test)]
mod schema_tests {
    use std::collections::BTreeSet;

    use super::*;

    /// The schema shipped beside the config describes every key a theme may write: a field added
    /// to `Theme` and forgotten here would leave an editor rejecting the theme a user just wrote,
    /// which is the drift this test exists to catch.
    #[test]
    fn the_schema_describes_every_key_a_theme_has() {
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../../theme.schema.json")).expect("the schema parses");

        let written = toml_edit::ser::to_string_pretty(&Theme::default()).expect("a theme writes");
        let theme: toml_edit::DocumentMut = written.parse().expect("and reads back");

        let properties = schema["properties"]
            .as_object()
            .expect("the schema has properties");

        for (key, value) in theme.as_table().iter() {
            // `name` is the theme's own field — the key it was filed under in `[themes.<name>]` —
            // and not something a theme file writes, which is why the schema has no `name`.
            if key == "name" {
                continue;
            }

            assert!(
                properties.contains_key(key),
                "`{key}` is a theme key the schema does not describe"
            );

            // A table in the theme (a section) is a section in the schema, with the same keys in
            // it: what the schema says about one level it has to say about the next.
            if let Some(section) = value.as_table() {
                let described = schema
                    .pointer(&format!("/properties/{key}/properties"))
                    .and_then(serde_json::Value::as_object)
                    .unwrap_or_else(|| panic!("`{key}` has no section in the schema"));

                for inner in section.iter().map(|(inner, _)| inner) {
                    assert!(
                        described.contains_key(inner),
                        "`{key}.{inner}` is a theme key the schema does not describe"
                    );
                }
            }
        }

        // And nothing is described that a theme cannot write.
        let written: BTreeSet<&str> = theme
            .as_table()
            .iter()
            .map(|(key, _)| key)
            .filter(|key| *key != "name")
            .collect();
        for described in properties.keys() {
            if described == "$schema" || described == "base" {
                continue;
            }
            assert!(
                written.contains(described.as_str()),
                "the schema describes `{described}`, which no theme has"
            );
        }
    }
}
