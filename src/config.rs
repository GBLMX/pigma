//! TOML configuration: the runtime `Config` plus the border/cache/column/
//! navigation/playerbar/theme registries.

mod audio;
mod border;
mod cache;
mod column;
pub mod keymap;
pub mod lyrics;
mod navigation;
mod notify;
mod panes;
mod playerbar;
mod symbols;
pub mod theme;
mod titles;

use std::{collections::HashMap, fs, path::Path};

pub use audio::*;
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
pub const CONFIG_VERSION: u32 = 2;

/// `config_version` is absent from files written before versioning existed, and such a
/// file must be treated as v0 rather than as "current" — otherwise a migration could
/// never trigger.
fn unversioned_config_version() -> u32 {
    0
}

/// Read the version-1 `proxy_target` out of a config file that is about to be
/// migrated. The key is gone from [`Config`], so it is read straight from the raw
/// TOML; a missing key — or a file that does not parse — means the old default,
/// `normal`.
fn legacy_proxy_target(config_path: &Path) -> Option<String> {
    let content = fs::read_to_string(config_path).ok()?;
    let doc = content.parse::<toml_edit::DocumentMut>().ok()?;
    doc.get("proxy_target")?.as_str().map(str::to_string)
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
    /// `[keys]`: which key runs which command, by the command's name — the command table's `key`
    /// column is the default, and this is what a user rebinds. Empty means "the table's own keys".
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub keys: crate::config::keymap::KeyBindings,
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
    /// `[audio]`: what the player does to the sound on its way to the device — sample-rate
    /// conversion, the parametric EQ and loudness normalization.
    #[serde(default)]
    pub audio: AudioConfig,
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
    /// Proxy address for every network request — the NetEase Cloud API, cover
    /// downloads and audio streams. Absent means a direct connection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
    /// Maximum number of search results.
    #[serde(default = "default_search_limit")]
    pub search_limit: u16,
    /// Navigation bar position: left (default), right, top, or bottom.
    #[serde(default)]
    pub navigation_position: NavPosition,
    /// Minimum splash screen display time (seconds); auto-transition waits for this even if boot finishes instantly.
    #[serde(default = "default_splash_duration")]
    pub splash_duration_secs: f64,
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

impl Default for Config {
    fn default() -> Self {
        Self {
            config_version: CONFIG_VERSION,
            keys: crate::config::keymap::KeyBindings::new(),
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
            proxy: None,
            search_limit: default_search_limit(),
            navigation_position: NavPosition::default(),
            splash_duration_secs: default_splash_duration(),
            logger: Logger::default(),
            cache: CacheConfig::default(),
            audio: AudioConfig::default(),
            playerbar: PlayerbarConfig::default(),
            titles: TitlesConfig::default(),
            themes: HashMap::new(),
            navigation: NavConfig::default(),
            columns: ColumnsConfig::default(),
            cli_status_template: default_cli_status_template(),
            cli_status_format: default_cli_status_format(),
        }
    }
}

impl Config {
    /// The lyrics page's options, resolved against the theme in force.
    ///
    /// The page is handed this rather than the whole config: it draws five things, and resolving
    /// the `ktv` colour here means once a frame rather than once per line, which is also what
    /// keeps an unknown colour to a single warning per name.
    pub fn lyrics_config(&self, theme: &Theme) -> crate::config::lyrics::LyricsConfig<'_> {
        crate::config::lyrics::LyricsConfig {
            style: self.lyric_style,
            gradient: self.lyric_gradient,
            ktv_color: theme.resolve_color(&self.lyric_ktv_color),
            show_translation: self.lyric_translation,
            title: &self.titles.lyrics,
        }
    }

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

        let (mut config, migrated) = match parsed {
            Some(mut cfg) => {
                let migrated = cfg.migrate_from(config_path);
                (cfg, migrated)
            }
            None => (Config::default(), false),
        };
        // A config read from the user's file is the user's: the app that loaded it may write it
        // back. (`Config::deserialize` cannot know this — `persist` is skipped on the way in —
        // so it is said here, and `App::new` narrows it again to "the process owns the
        // terminal".)
        config.persist = true;

        // Write the file back when this call changed it: there was no file (a fresh default),
        // or `migrate_from` just upgraded one (after the backup it took). Doing it here rather
        // than at the next `save()` is what makes a migration visible at once — the keys the
        // new schema dropped stop sitting in the user's file — and it is why the write targets
        // the path that was loaded rather than `save()`'s own directory.
        //
        // A file that is already current is left byte-for-byte alone, which is what keeps a
        // user's own comments, and a file that exists but failed to parse is left alone too:
        // overwriting it with defaults would silently discard what they wrote.
        let fresh = !config_path.exists();
        if fresh || migrated {
            if fresh && let Some(dir) = config_path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let content = config.to_toml();
            if content.is_empty() {
                log::error!("Refusing to overwrite config.toml with an empty document");
            } else if let Err(e) = fs::write(config_path, content) {
                log::warn!("Failed to write config.toml: {e}");
            }
        }
        config
    }

    /// Upgrade a config parsed from disk to [`CONFIG_VERSION`].
    ///
    /// The previous file is copied to `config.toml.bak-v{old}` first, so a user can always
    /// roll back. Returns whether anything was upgraded, which is what tells the caller to
    /// write the migrated file back at once: a migration the user cannot see on disk has not
    /// happened as far as the next reader of that file is concerned. A file from a newer
    /// build is left as it is (its unknown fields are ignored) instead of being downgraded,
    /// and reports `false` so it is not written either.
    fn migrate_from(&mut self, config_path: &Path) -> bool {
        let from = self.config_version;
        if from == CONFIG_VERSION {
            return false;
        }
        if from > CONFIG_VERSION {
            log::warn!(
                "config.toml is schema v{from}, this build understands v{CONFIG_VERSION}: \
                 newer fields are ignored"
            );
            return false;
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

        // v1 → v2: `proxy_target` is gone along with the multi-source fallback it
        // scoped, and `proxy` now covers every network source. `normal` (the default)
        // only ever proxied one non-NetEase source — the source that no longer exists —
        // so that address is dropped and the user ends up on a direct connection;
        // `reversed`/`both` meant "proxy NetEase traffic", which is exactly what
        // `proxy` means now, so it is kept.
        if from < 2 {
            match legacy_proxy_target(config_path).as_deref() {
                None | Some("normal") => {
                    self.proxy = None;
                    log::info!(
                        "config.toml schema v1 → v2: proxy_target 为默认值，不再代理（清理 proxy）"
                    );
                }
                Some(target) => log::info!(
                    "config.toml schema v1 → v2: 保留 proxy（原 proxy_target = {target}）"
                ),
            }
        }

        self.config_version = CONFIG_VERSION;

        true
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

    pub(crate) fn to_toml(&self) -> String {
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
        let rewritten = fs::read_to_string(&path).expect("read migrated config");
        assert!(
            rewritten.contains(&format!("config_version = {CONFIG_VERSION}")),
            "the migrated file must be written back at startup, not at the next save:\n{rewritten}"
        );
        assert!(
            fs::read_to_string(dir.join("config.toml.bak-v0"))
                .expect("read the backup")
                .contains("search_limit = 42"),
            "the backup has to hold the file as it was, or it is not a rollback"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// The v1 → v2 rule: the old `proxy_target` decides whether the configured `proxy`
    /// survives. `normal` only ever proxied a source that no longer exists — never
    /// NetEase — so the address is dropped and the user ends up direct;
    /// `reversed`/`both` meant NetEase traffic goes through the proxy, which is what
    /// `proxy` means now, so it is kept. Neither the stale key nor the (removed)
    /// `[source_fallback]` section may be written back.
    #[test]
    fn v1_proxy_target_decides_whether_the_proxy_survives() {
        for (target, expected) in [
            ("normal", None),
            ("both", Some("http://127.0.0.1:7890")),
            ("reversed", Some("http://127.0.0.1:7890")),
        ] {
            let dir = scratch_dir(&format!("proxy-target-{target}"));
            let path = dir.join("config.toml");
            fs::write(
                &path,
                format!(
                    "config_version = 1\n\
                     proxy = \"http://127.0.0.1:7890\"\n\
                     proxy_target = \"{target}\"\n\
                     [source_fallback]\n\
                     enabled = true\n"
                ),
            )
            .expect("write v1 config");

            let cfg = Config::load_from(&path);

            assert_eq!(cfg.config_version, CONFIG_VERSION);
            assert_eq!(cfg.proxy.as_deref(), expected, "proxy_target = {target}");
            assert!(
                dir.join("config.toml.bak-v1").exists(),
                "the v1 file must be backed up"
            );
            let on_disk = fs::read_to_string(&path).expect("read the migrated config");
            assert!(
                on_disk.contains(&format!("config_version = {CONFIG_VERSION}")),
                "the file must be brought to the new schema at startup:\n{on_disk}"
            );
            assert!(
                !on_disk.contains("proxy_target") && !on_disk.contains("source_fallback"),
                "the keys the new schema dropped must be gone from the user's file:\n{on_disk}"
            );
            let backup = fs::read_to_string(dir.join("config.toml.bak-v1")).expect("read backup");
            assert!(
                backup.contains("proxy_target") && backup.contains("[source_fallback]"),
                "the backup is the rollback path and must keep the v1 file verbatim:\n{backup}"
            );
            let _ = fs::remove_dir_all(&dir);
        }
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

    /// A file already at the current schema is not rewritten: it is the user's file, and what
    /// they wrote in it — comments included — is part of it. Only a migration overwrites it,
    /// and only after taking a backup.
    #[test]
    fn a_current_config_is_left_byte_for_byte_alone() {
        let dir = scratch_dir("current-untouched");
        let path = dir.join("config.toml");
        let authored = format!(
            "# 手写的一行注释：重写就会丢\n\
             config_version = {CONFIG_VERSION}\n\
             default_theme = \"dracula\"\n\
             search_limit = 7\n"
        );
        fs::write(&path, &authored).expect("write current config");

        let cfg = Config::load_from(&path);

        assert_eq!(cfg.search_limit, 7, "the value must still be read");
        assert_eq!(
            fs::read_to_string(&path).expect("read config"),
            authored,
            "a config that was not migrated must not be rewritten"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// A file from a newer build is used as it is and never written back: this build does not
    /// understand its fields, and rewriting it would drop them.
    #[test]
    fn a_newer_config_is_read_but_never_rewritten() {
        let dir = scratch_dir("newer");
        let path = dir.join("config.toml");
        let authored = format!(
            "config_version = {}\nsearch_limit = 9\nfuture_key = \"kept\"\n",
            CONFIG_VERSION + 1
        );
        fs::write(&path, &authored).expect("write newer config");

        let cfg = Config::load_from(&path);

        assert_eq!(cfg.search_limit, 9, "the fields this build knows are read");
        assert_eq!(
            fs::read_to_string(&path).expect("read config"),
            authored,
            "a file from a newer schema must be left alone, not downgraded"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
