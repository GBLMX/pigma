use std::{sync::Arc, time::Duration};

use ratatui_image::picker::Picker;
use reqwest::Client;
use sonar::SonarFinder;

use super::App;
use crate::{
    config::{Config, ProxyTarget, ThemeRegistry},
    state::{COMMANDS, CommandItem, CommandPanel},
    utils::terminal::{ImageProtocol, choose_image_protocol},
};

/// Deadline for the connection phase (TCP + TLS, through the proxy when one is
/// configured). An unreachable proxy or CDN host must fail, not hang the task.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Deadline for a single read: it restarts every time reqwest hands out a chunk,
/// so it aborts a stalled socket and never caps how long a transfer may run.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Total deadline for one-shot requests (cover downloads). Never applied to the
/// streaming client: `stream_download` holds one response body open for the
/// length of a track, and a wall-clock deadline would cut playback short.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Used to decide whether a proxy is enabled based on `ProxyTarget`.
pub(super) enum ProxyKind {
    /// Non-YouTube services (NetEase Cloud, sonar search, covers, streaming), proxied under `Reversed`/`Both`.
    NonYoutube,
    /// YouTube services, proxied under `Normal`/`Both`.
    Youtube,
}

impl App {
    /// `NonYoutube` is proxied under `Reversed`/`Both`; `Youtube` under `Normal`/`Both`.
    pub(super) fn proxy_for(config: &Config, kind: ProxyKind) -> &str {
        let proxy = config.proxy.as_str();
        if proxy.is_empty() {
            return "";
        }
        let active = match kind {
            ProxyKind::NonYoutube => {
                matches!(
                    config.proxy_target,
                    ProxyTarget::Reversed | ProxyTarget::Both
                )
            }
            ProxyKind::Youtube => {
                matches!(config.proxy_target, ProxyTarget::Normal | ProxyTarget::Both)
            }
        };
        if active { proxy } else { "" }
    }

    /// Build the command palette from the one command table.
    ///
    /// Every entry comes from [`COMMANDS`], so the palette cannot drift from the `:` command
    /// line again: the same text that names a command there is what Enter runs here. Themes are
    /// the one entry rendered as a submenu, because the list comes from the registry at runtime.
    pub(super) fn build_command_panel(theme_registry: &ThemeRegistry) -> CommandPanel {
        let theme_children: Vec<CommandItem> = theme_registry
            .choosable_names()
            .into_iter()
            .map(|name| CommandItem::Action {
                name: name.to_string(),
                summary: "",
                key: None,
                ex: format!("theme {name}"),
                needs_argument: false,
            })
            .collect();

        let mut commands: Vec<CommandItem> = vec![CommandItem::SubMenu {
            name: "切换主题".to_string(),
            children: theme_children,
        }];

        commands.extend(
            COMMANDS
                .iter()
                .filter(|command| command.name != "theme" && command.in_palette)
                .map(|command| CommandItem::Action {
                    name: command.name.to_string(),
                    summary: command.summary,
                    key: command.key,
                    ex: command.ex.to_string(),
                    needs_argument: command.needs_argument,
                }),
        );

        let mut command_panel = CommandPanel::new();
        command_panel.levels = vec![commands];
        command_panel
    }

    /// Build the sonar finder per config, applying the search/YouTube proxy.
    pub(super) fn build_finder(
        config: &Config,
        search_proxy: &str,
        youtube_proxy: &str,
    ) -> color_eyre::Result<Arc<SonarFinder>> {
        let mut sources: Vec<sonar::SonarSource> = Vec::new();
        for name in &config.source_fallback.providers {
            match sonar::SonarSource::from_name(name) {
                Some(source) if !sources.contains(&source) => sources.push(source),
                Some(_) => {}
                None => log::warn!(
                    "unknown [source_fallback] provider {name:?}; expected one of {}",
                    sonar::SonarSource::ALL
                        .iter()
                        .map(sonar::SonarSource::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        }
        if sources.is_empty() {
            // A typo in the provider list must not silently disable the fallback
            // sources; fall back to the defaults and say so.
            log::warn!("no valid [source_fallback] providers configured; using defaults");
            sources = sonar::SonarSource::ALL.to_vec();
        }
        let search_config = sonar::SearchConfig::new()
            .with_providers(sources)
            .with_timeout(config.source_fallback.timeout_ms)
            .with_search_proxy(search_proxy.to_string())
            .with_youtube_proxy(youtube_proxy.to_string());
        let finder = sonar::SonarFinder::new(search_config).map_err(color_eyre::Report::msg)?;
        Ok(Arc::new(finder))
    }

    /// Build the image picker for the current terminal and settle on a protocol.
    ///
    /// The picker queries the terminal itself — graphics support and the cell size in
    /// pixels — and that answer is what [`choose_image_protocol`] trusts first; the
    /// configuration can override the result. The decision is logged, because "the
    /// cover looks wrong" is otherwise impossible to answer from a bug report.
    pub(super) fn build_picker(playerbar: &crate::config::PlayerbarConfig) -> Picker {
        use ratatui_image::picker::{Picker, ProtocolType};

        // A terminal that does not answer falls back to half blocks, which is what
        // upstream recommends: the fixed-cell-size constructors are deprecated, and a
        // guessed cell size would scale every cover wrongly anyway.
        let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

        let queried = match picker.protocol_type() {
            ProtocolType::Kitty => Some(ImageProtocol::Kitty),
            ProtocolType::Iterm2 => Some(ImageProtocol::ITerm2),
            ProtocolType::Sixel => Some(ImageProtocol::Sixel),
            _ => None,
        };

        let tmux = picker.tmux_detected();
        let chosen = choose_image_protocol(playerbar.image_protocol, queried, tmux, |key| {
            std::env::var(key).ok()
        });

        log::debug!(
            "covers: config={:?} queried={queried:?} tmux={tmux} -> {chosen:?}",
            playerbar.image_protocol
        );

        picker.set_protocol_type(match chosen {
            Some(ImageProtocol::Kitty) => ProtocolType::Kitty,
            Some(ImageProtocol::ITerm2) => ProtocolType::Iterm2,
            Some(ImageProtocol::Sixel) => ProtocolType::Sixel,
            None => ProtocolType::Halfblocks,
        });
        picker
    }

    /// Build the client that streams audio; `proxy` is the streaming proxy (empty = direct).
    ///
    /// Only the connect and per-read deadlines apply. A total deadline would abort
    /// playback mid-track: `stream_download` keeps a single response body open and
    /// reads from it for as long as the track plays, so the only legitimate limit is
    /// "no byte within [`READ_TIMEOUT`]" — which a stalled socket trips and a playing
    /// track never does.
    pub(super) fn build_stream_client(proxy: &str) -> color_eyre::Result<Client> {
        Self::http_client_builder(proxy)?
            .build()
            .map_err(color_eyre::Report::msg)
    }

    /// Build the client for one-shot requests (cover downloads); `proxy` is the search/cover proxy (empty = direct).
    ///
    /// The whole body is consumed at once here, so the total deadline of
    /// [`REQUEST_TIMEOUT`] is safe in addition to the shared connect/read deadlines —
    /// a cover download that never completes must not keep its spawned task alive.
    pub(super) fn build_cover_client(proxy: &str) -> color_eyre::Result<Client> {
        Self::http_client_builder(proxy)?
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(color_eyre::Report::msg)
    }

    /// Proxy plus the connect/read deadlines every client shares; the caller decides
    /// whether a total deadline applies (streaming must not have one).
    fn http_client_builder(proxy: &str) -> color_eyre::Result<reqwest::ClientBuilder> {
        let mut builder = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT);
        if !proxy.is_empty() {
            builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(color_eyre::Report::msg)?);
        }
        Ok(builder)
    }
}

#[cfg(test)]
mod command_panel_tests {
    use super::*;
    use crate::state::COMMANDS;

    /// The palette is built from the table, so it cannot fall behind it: every command the `:`
    /// line knows has an entry (themes are the submenu above them).
    #[test]
    fn the_palette_lists_every_command() {
        let registry = ThemeRegistry::new(Default::default());
        let panel = App::build_command_panel(&registry);
        let items = panel.current_items().expect("built with one level");

        let listed: Vec<&str> = items
            .iter()
            .filter_map(|item| match item {
                CommandItem::Action { name, .. } => Some(name.as_str()),
                CommandItem::SubMenu { .. } => None,
            })
            .collect();

        let expected: Vec<&str> = COMMANDS
            .iter()
            .filter(|c| c.in_palette && c.name != "theme")
            .map(|c| c.name)
            .collect();
        for command in expected
            .iter()
            .map(|name| COMMANDS.iter().find(|c| c.name == *name).unwrap())
        {
            assert!(
                listed.contains(&command.name),
                "{} is missing from the palette: {listed:?}",
                command.name
            );
        }
        assert_eq!(
            listed.len(),
            expected.len(),
            "the palette and the table disagree: {listed:?}"
        );
    }

    /// Themes come from the registry at runtime, and picking one has to run `theme <name>`.
    #[test]
    fn the_theme_submenu_runs_the_theme_command() {
        let registry = ThemeRegistry::new(Default::default());
        let panel = App::build_command_panel(&registry);
        let Some(CommandItem::SubMenu { children, .. }) = panel.current_items().unwrap().first()
        else {
            panic!("the first entry is the theme submenu");
        };
        let first = children.first().expect("at least one theme");
        let CommandItem::Action { ex, .. } = first else {
            panic!("theme entries are actions");
        };
        assert!(ex.starts_with("theme "), "{ex}");
        crate::input::ex::ExCommand::parse(ex).expect("the submenu entry parses");
    }
}
