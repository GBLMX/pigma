//! TOML configuration: the runtime `Config` plus the border/cache/column/
//! navigation/playerbar/theme registries.

mod border;
mod cache;
mod column;
mod navigation;
mod playerbar;
mod symbols;
pub mod theme;
mod titles;

use std::{fs, path::Path};

pub use border::*;
pub use cache::*;
pub use column::*;
pub use navigation::*;
pub use playerbar::*;
use serde::{Deserialize, Serialize};
pub use symbols::*;
pub use theme::{Theme, ThemeRegistry, theme_fallback};
pub use titles::*;

use crate::{
    logger::Logger,
    utils::{self, GradientPreset, terminal::BackgroundMode},
};

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
    /// Schema version of the file this config was loaded from; written back on save.
    #[serde(default = "unversioned_config_version")]
    pub config_version: u32,
    pub default_theme: String,
    /// Theme used when the terminal background is light; `default_theme` is the dark slot.
    #[serde(default)]
    pub light_theme: Option<String>,
    /// Which background the theme slots are chosen for: `auto` (follow the terminal),
    /// `dark`, or `light`.
    #[serde(default)]
    pub background: BackgroundMode,
    /// Glyph preset and per-key overrides for terminals without a Nerd Font.
    #[serde(default)]
    pub symbols: SymbolsConfig,
    pub border: BorderConfig,
    pub seek_interval_secs: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub themes: Vec<Theme>,
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
    /// Default template for `pigma status` (plain format).
    #[serde(default = "default_cli_status_template")]
    pub cli_status_template: String,
    /// Default format for `pigma status`: `plain` or `json`.
    #[serde(default = "default_cli_status_format")]
    pub cli_status_format: String,
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
            default_theme: Theme::default().name,
            light_theme: None,
            background: BackgroundMode::default(),
            symbols: SymbolsConfig::default(),
            border: BorderConfig::default(),
            seek_interval_secs: 15,
            lyric_gradient: GradientPreset::default(),
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
            themes: Vec::new(),
            navigation: NavConfig::default(),
            columns: ColumnsConfig::default(),
            cli_status_template: default_cli_status_template(),
            cli_status_format: default_cli_status_format(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        Self::load_from(&utils::pigma_config_dir().join("config.toml"))
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

        let config = if let Some(mut cfg) = parsed {
            cfg.migrate_from(config_path);
            cfg
        } else {
            Config::default()
        };

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
        let dir = utils::pigma_config_dir();
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
        let dir =
            std::env::temp_dir().join(format!("pigma-config-test-{}-{label}", std::process::id()));
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
