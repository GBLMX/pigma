use std::{collections::HashMap, str::FromStr, sync::LazyLock};

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::utils::terminal::{COLOR_MODE, ColorMode, rgb_to_16, rgb_to_256};

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
        }
    }

    /// Convert every true-color token to the terminal's palette.
    ///
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
            error: downsample_color(self.error, mode),
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
                log::warn!("Unknown theme field: \"{name}\", falling back to accent");
                self.accent
            }
        }
    }
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

    pub fn all_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.extras.keys().map(|s| s.as_str()).collect();
        for k in builtin_themes().keys() {
            if !self.extras.contains_key(k) {
                names.push(k);
            }
        }
        names
    }
}

/// Last-resort hardcoded theme used when neither the configured theme nor the
/// built-in `default` theme is available.
pub fn theme_fallback() -> &'static Theme {
    static FALLBACK: LazyLock<Theme> = LazyLock::new(|| Theme::default().downsampled(*COLOR_MODE));
    &FALLBACK
}

#[cfg(test)]
mod user_theme_tests {
    use super::*;

    /// Go through the real loader: a config file carrying a `[themes.<name>]` table has to
    /// load at all, which is what used to fail for the documented hex colours.
    fn user(label: &str, source: &str) -> HashMap<String, UserTheme> {
        let dir =
            std::env::temp_dir().join(format!("pigma-theme-test-{}-{label}", std::process::id()));
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
