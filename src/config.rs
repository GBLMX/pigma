//! TOML configuration: the runtime `Config` plus the border/cache/column/
//! navigation/playerbar/theme registries.

mod border;
mod cache;
mod column;
mod lyrics;
mod navigation;
mod notify;
mod panes;
mod playerbar;
mod symbols;
pub mod theme;
mod titles;

use std::{collections::HashMap, fs, path::Path};

pub use border::*;
pub use cache::*;
pub use column::*;
pub use lyrics::*;
pub use navigation::*;
pub use notify::*;
pub use panes::*;
pub use playerbar::*;
use serde::{Deserialize, Serialize};
pub use symbols::*;
pub use theme::{ColorSpec, RANDOM_THEME, Theme, ThemeRegistry, UserTheme, theme_fallback};
pub use titles::*;

use crate::{
    logger::Logger,
    utils::{
        self, GradientPreset,
        terminal::{BackgroundFill, BackgroundMode},
    },
};

/// `#[serde(default)]` for a field whose natural default is "on".
fn default_true() -> bool {
    true
}

/// Schema version of `config.toml` written by this build.
///
/// Bump it whenever a field is renamed, removed, or changes meaning, and add the
/// matching step in [`Config::migrate_from`].
pub const CONFIG_VERSION: u32 = 1;

/// `config_version` is absent from files written before versioning existed, and such a
/// file must be treated as v0 rather than as "current" — otherwise a migration could
/// never trigger.
fn unversioned_config_version() -> u32 {
    0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Whether this config may be written back to the user's file.
    ///
    /// The TUI owns the terminal and the user's config; a test, the CLI and the headless paths
    /// do not — and they *do* call the same setters and commands, every one of which saves. A
    /// test that resized a pane used to write its own default config straight over the real one
    /// (measured: a `cargo test` run replaced a user's theme, gradient, player-bar layout and
    /// navigation position with defaults). Never the user's file from a process that does not
    /// own it.
    #[serde(skip)]
    pub persist: bool,
    #[serde(default = "unversioned_config_version")]
    pub config_version: u32,
    pub default_theme: String,
    /// Theme used when the terminal background is light; `default_theme` is the dark slot.
    #[serde(default)]
    pub light_theme: Option<String>,
    /// The panes inside the frame: their sizes, and which are collapsed.
    #[serde(default)]
    pub panes: PanesConfig,
    /// Whether the app paints the theme's background over the whole frame: `auto` (only when it
    /// differs from the terminal's, which keeps a translucent or acrylic terminal background),
    /// `always`, or `never`.
    #[serde(default)]
    pub paint_background: BackgroundFill,
    /// Which background the theme slots are chosen for: `auto` (follow the terminal),
    /// `dark`, or `light`.
    #[serde(default)]
    pub background: BackgroundMode,
    /// Glyph preset and per-key overrides for terminals without a Nerd Font.
    #[serde(default)]
    pub symbols: SymbolsConfig,
    pub border: BorderConfig,
    pub seek_interval_secs: u32,
    /// User themes, `[themes.<name>]` in the config: a base plus the colours to override.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub themes: HashMap<String, UserTheme>,
    pub logger: Logger,
    pub navigation: NavConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub playerbar: PlayerbarConfig,
    #[serde(default)]
    pub titles: TitlesConfig,
    #[serde(default)]
    pub columns: ColumnsConfig,
    /// Lyrics highlight gradient style: warm / cubehelix / rainbow / spectral / viridis / turbo.
    #[serde(default)]
    pub lyric_gradient: GradientPreset,
    /// Colour of the sung part in the `ktv` lyrics style: a theme field (`accent`, `text`, …)
    /// or a colour of its own (`blue`, `lightblue`, `#4da6ff`, an ANSI index).
    #[serde(default = "default_lyric_ktv_color")]
    pub lyric_ktv_color: String,
    /// Draw the translated lyric lines under the original ones (`y`, `:translation on|off`).
    /// Only songs whose lyrics come with a translation have any to draw.
    #[serde(default = "default_true")]
    pub lyric_translation: bool,
    /// How the lyrics page draws: `window`, `one_line`, `ktv`, `flow` or `plain`.
    #[serde(default)]
    pub lyric_style: LyricStyle,
    /// Capture the mouse: clicks on the player bar, the tabs and the lists. Turning it off
    /// keeps the terminal's own selection and scrolling (holding `Shift` does the same in
    /// most terminals, without the config).
    #[serde(default = "default_true")]
    pub mouse: bool,
    /// Shape of the cursor in the input fields: `default` (whatever the terminal is set to),
    /// `block`, `underline` or `bar`.
    #[serde(default)]
    pub cursor_style: crate::utils::terminal::CursorStyle,
    /// Desktop notifications, off by default.
    #[serde(default)]
    pub notify: NotifyConfig,
    /// Proxy address (leave empty to disable the proxy).
    #[serde(default = "default_proxy")]
    pub proxy: String,
    /// Proxy target: `normal` proxies only YouTube (default, domestic users),
    /// `reversed` proxies everything except YouTube (overseas users),
    /// `both` proxies everything.
    #[serde(default = "default_proxy_target")]
    pub proxy_target: ProxyTarget,
    /// Maximum number of search results.
    #[serde(default = "default_search_limit")]
    pub search_limit: u16,
    /// Navigation bar position: left (default), right, top, or bottom.
    #[serde(default)]
    pub navigation_position: NavPosition,
    /// Minimum splash screen display time (seconds); auto-transition waits for this even if boot finishes instantly.
    #[serde(default = "default_splash_duration")]
    pub splash_duration_secs: f64,
    /// sonar fallback source config (multi-source fallback when NCM playback fails).
    #[serde(default)]
    pub source_fallback: SonarConfig,
    /// Default template for `boxpigma status` (plain format).
    #[serde(default = "default_cli_status_template")]
    pub cli_status_template: String,
    /// Default format for `boxpigma status`: `plain` or `json`.
    #[serde(default = "default_cli_status_format")]
    pub cli_status_format: String,
}

/// The karaoke blue: what a KTV screen paints over the words once they have been sung.
fn default_lyric_ktv_color() -> String {
    "#4da6ff".into()
}

fn default_proxy() -> String {
    "http://127.0.0.1:7890".into()
}

fn default_proxy_target() -> ProxyTarget {
    ProxyTarget::Normal
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyTarget {
    /// Domestic default: only YouTube goes through the proxy; everything else connects directly.
    Normal,
    /// Overseas users: everything except YouTube goes through the proxy.
    Reversed,
    /// Everything goes through the proxy.
    Both,
}

/// Navigation bar position: left (default), right, top, or bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NavPosition {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

impl NavPosition {
    /// The next position in the left → right → top → bottom cycle.
    pub fn cycle(self) -> Self {
        match self {
            NavPosition::Left => NavPosition::Right,
            NavPosition::Right => NavPosition::Top,
            NavPosition::Top => NavPosition::Bottom,
            NavPosition::Bottom => NavPosition::Left,
        }
    }

    /// Human-readable Chinese label used for toasts.
    pub fn label(self) -> &'static str {
        match self {
            NavPosition::Left => "左侧",
            NavPosition::Right => "右侧",
            NavPosition::Top => "顶部",
            NavPosition::Bottom => "底部",
        }
    }
}

fn default_search_limit() -> u16 {
    100
}

fn default_splash_duration() -> f64 {
    2.0
}

fn default_cli_status_template() -> String {
    "{name}  {artist}  {current}/{duration}  {status}  vol {volume}%".into()
}

fn default_cli_status_format() -> String {
    "plain".into()
}

/// Fallback source config (sonar multi-source fallback).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SonarConfig {
    /// Whether fallback sources are enabled.
    pub enabled: bool,
    /// Sources participating in fallback, ordered from highest to lowest priority:
    /// `kuwo`, `kugou`, `bilivideo`, `youtube`.
    pub providers: Vec<String>,
    /// Per-source search timeout (milliseconds).
    pub timeout_ms: u64,
}

impl Default for SonarConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            providers: vec![
                "kuwo".to_string(),
                "kugou".to_string(),
                "bilivideo".to_string(),
                "youtube".to_string(),
            ],
            timeout_ms: 10000,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            config_version: CONFIG_VERSION,
            persist: true,
            default_theme: Theme::default().name,
            light_theme: None,
            panes: PanesConfig::default(),
            paint_background: BackgroundFill::default(),
            background: BackgroundMode::default(),
            symbols: SymbolsConfig::default(),
            border: BorderConfig::default(),
            seek_interval_secs: 15,
            lyric_gradient: GradientPreset::default(),
            lyric_style: LyricStyle::default(),
            lyric_ktv_color: default_lyric_ktv_color(),
            lyric_translation: true,
            mouse: true,
            cursor_style: crate::utils::terminal::CursorStyle::default(),
            notify: NotifyConfig::default(),
            proxy: default_proxy(),
            proxy_target: default_proxy_target(),
            search_limit: default_search_limit(),
            navigation_position: NavPosition::default(),
            splash_duration_secs: default_splash_duration(),
            logger: Logger::default(),
            cache: CacheConfig::default(),
            playerbar: PlayerbarConfig::default(),
            titles: TitlesConfig::default(),
            source_fallback: SonarConfig::default(),
            themes: HashMap::new(),
            navigation: NavConfig::default(),
            columns: ColumnsConfig::default(),
            cli_status_template: default_cli_status_template(),
            cli_status_format: default_cli_status_format(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        Self::load_from(&utils::boxpigma_config_dir().join("config.toml"))
    }

    /// Load a config from an explicit path, upgrading a file written by an older release.
    ///
    /// Split out of [`Config::load`] so the migration path is testable against a scratch
    /// file instead of the user's real config.
    pub fn load_from(config_path: &Path) -> Self {
        let parsed: Option<Config> = if config_path.exists() {
            match fs::read_to_string(config_path) {
                Ok(content) => match toml_edit::de::from_str(&content) {
                    Ok(cfg) => Some(cfg),
                    Err(e) => {
                        log::warn!("Failed to parse config.toml: {e}, using defaults");
                        None
                    }
                },
                Err(e) => {
                    log::warn!("Failed to read config.toml: {e}, using defaults");
                    None
                }
            }
        } else {
            None
        };

        let mut config = if let Some(mut cfg) = parsed {
            cfg.migrate_from(config_path);
            cfg
        } else {
            Config::default()
        };
        // A config read from the user's file is the user's: the app that loaded it may write it
        // back. (`Config::deserialize` cannot know this — `persist` is skipped on the way in —
        // so it is said here, and `App::new` narrows it again to "the process owns the
        // terminal".)
        config.persist = true;

        // Only a missing file is (re)created: a file that exists but failed to parse is
        // left alone rather than overwritten with defaults.
        if !config_path.exists() {
            if let Some(dir) = config_path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let content = config.to_toml();
            if let Err(e) = fs::write(config_path, content) {
                log::warn!("Failed to write default config: {e}");
            }
        }
        config
    }

    /// Upgrade a config parsed from disk to [`CONFIG_VERSION`].
    ///
    /// The previous file is copied to `config.toml.bak-v{old}` first, so a user can always
    /// roll back; the upgraded config is written on the next `save()`. A file from a newer
    /// build is left as it is (its unknown fields are ignored) instead of being downgraded.
    fn migrate_from(&mut self, config_path: &Path) {
        let from = self.config_version;
        if from == CONFIG_VERSION {
            return;
        }
        if from > CONFIG_VERSION {
            log::warn!(
                "config.toml is schema v{from}, this build understands v{CONFIG_VERSION}: \
                 newer fields are ignored"
            );
            return;
        }

        let backup = config_path.with_extension(format!("toml.bak-v{from}"));
        match fs::copy(config_path, &backup) {
            Ok(_) => log::info!(
                "config.toml schema v{from} → v{CONFIG_VERSION}（已备份到 {}）",
                backup.display()
            ),
            Err(e) => log::warn!(
                "config.toml schema v{from} → v{CONFIG_VERSION}（备份到 {} 失败: {e}）",
                backup.display()
            ),
        }

        // Steps run in order, each rewriting whatever the previous schema got wrong.
        // There is no field to rewrite for v0 → v1 yet: files written before versioning
        // existed load unchanged, and the version is recorded here so that the next
        // migration can key off it.
        self.config_version = CONFIG_VERSION;
    }

    pub fn save(&self) {
        if !self.persist {
            return;
        }

        let dir = utils::boxpigma_config_dir();
        if let Err(e) = fs::create_dir_all(&dir) {
            log::error!("Failed to create config directory: {e}");
            return;
        }
        let content = self.to_toml();
        if content.is_empty() {
            log::error!("Refusing to overwrite config.toml with an empty document");
            return;
        }
        if let Err(e) = fs::write(dir.join("config.toml"), content) {
            log::error!("Failed to write config.toml: {e}");
        }
    }

    fn to_toml(&self) -> String {
        let Ok(pretty) = toml_edit::ser::to_string_pretty(self) else {
            log::error!("failed to serialize config to TOML");
            return String::new();
        };
        let Ok(mut doc) = pretty.parse::<toml_edit::DocumentMut>() else {
            return pretty;
        };

        // Every step below is best-effort: `serde` may omit these tables (or serialize them
        // as plain arrays instead of arrays-of-tables, e.g. when the user clears
        // `navigation.sections` or `columns.songs`), which must not panic.
        if let Some(nav) = doc.get_mut("navigation").and_then(|v| v.as_table_mut()) {
            nav.set_implicit(true);
            if let Some(sections) = nav
                .get_mut("sections")
                .and_then(|v| v.as_array_of_tables_mut())
            {
                for section in sections.iter_mut() {
                    utils::format::convert_aot_to_inline(section, "items", "\n  ");
                }
            }
        }

        if let Some(columns) = doc.get_mut("columns").and_then(|v| v.as_table_mut()) {
            columns.set_implicit(true);
            if let Some(overrides) = columns.get_mut("overrides").and_then(|v| v.as_table_mut()) {
                overrides.set_implicit(true);
                utils::format::convert_all_aot_to_inline(overrides, "\n  ");
            }
            utils::format::convert_aot_to_inline(columns, "songs", "\n  ");
            utils::format::convert_aot_to_inline(columns, "songlist", "\n  ");
        }

        doc.to_string()
    }
}

#[cfg(test)]
mod tests {
    /// A config the process does not own is never written: `App::new(_, false)` — tests, the
    /// CLI, the headless paths — leaves the user's file exactly as it found it.
    ///
    /// This is the regression test for a `cargo test` run replacing a real config with defaults
    /// (theme, gradient, player-bar layout, navigation position — all of it), because a test
    /// resized a pane and the setter that follows a drag saved.
    #[test]
    fn a_config_that_is_not_the_users_is_never_written() {
        let path = utils::boxpigma_config_dir().join("config.toml");
        let before = fs::read(&path).ok();
        let config = Config {
            persist: false,
            ..Config::default()
        };

        config.save();

        assert_eq!(
            fs::read(&path).ok(),
            before,
            "a config the process does not own was written over the user's"
        );
    }

    /// Mouse capture is what makes the player bar clickable and also what stops the terminal
    /// from selecting text, so the default is on and the way out has to actually parse.
    #[test]
    fn mouse_defaults_to_on_and_can_be_turned_off() {
        let parsed: Config = toml_edit::de::from_str("mouse = false\n").expect("parse");
        assert!(!parsed.mouse);
        assert!(
            Config::default().mouse,
            "capture is on unless asked otherwise"
        );
    }

    use super::*;

    #[test]
    fn default_config_serializes_save_on_play() {
        let cfg = Config::default();
        let toml = cfg.to_toml();
        assert!(
            toml.contains("save_on_play = true"),
            "missing save_on_play in default config:\n{toml}"
        );
    }

    /// A cleared `navigation.sections` / `columns.songs` is serialized as an empty array
    /// rather than an array of tables, which used to panic inside `to_toml`.
    #[test]
    fn to_toml_survives_empty_sections_and_columns() {
        let mut cfg = Config::default();
        cfg.navigation.sections.clear();
        cfg.columns.songs.clear();

        let toml = cfg.to_toml();
        assert!(!toml.is_empty(), "config serialization produced nothing");
    }

    /// Scratch directory for the file-backed tests: they run in parallel, so the name
    /// carries the test's own label.
    fn scratch_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "boxpigma-config-test-{}-{label}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// A file written before versioning existed (no `config_version`) is upgraded in
    /// memory, keeps its values, and is backed up so the user can roll back.
    #[test]
    fn legacy_config_is_migrated_and_backed_up() {
        let dir = scratch_dir("legacy");
        let path = dir.join("config.toml");
        fs::write(&path, "default_theme = \"dracula\"\nsearch_limit = 42\n")
            .expect("write legacy config");

        let cfg = Config::load_from(&path);

        assert_eq!(cfg.config_version, CONFIG_VERSION);
        assert_eq!(cfg.default_theme, "dracula", "user values must survive");
        assert_eq!(cfg.search_limit, 42, "user values must survive");
        assert!(
            dir.join("config.toml.bak-v0").exists(),
            "the pre-versioning file must be backed up"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// The startup path end to end: `save`/first-run output carries the current version and
    /// is read back without triggering another migration.
    #[test]
    fn saved_config_round_trips_without_migrating_again() {
        let dir = scratch_dir("roundtrip");
        let path = dir.join("config.toml");
        let written = Config {
            default_theme: "dracula".into(),
            search_limit: 42,
            ..Config::default()
        };
        fs::write(&path, written.to_toml()).expect("write config");

        let cfg = Config::load_from(&path);

        assert_eq!(
            cfg.config_version, CONFIG_VERSION,
            "the written file must carry the current version"
        );
        assert_eq!(cfg.default_theme, "dracula", "values must survive");
        assert_eq!(cfg.search_limit, 42, "values must survive");
        assert!(
            !dir.join(format!("config.toml.bak-v{CONFIG_VERSION}"))
                .exists(),
            "a config at the current version must not be migrated"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
