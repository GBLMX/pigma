use std::sync::Arc;

use ratatui_image::picker::Picker;
use reqwest::Client;
use sonar::SonarFinder;

use super::App;
use crate::{
    config::{Config, ProxyTarget, ThemeRegistry},
    state::{COMMANDS, CommandItem, CommandPanel},
    utils::terminal::{ImageProtocol, choose_image_protocol},
};

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
            .all_names()
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

    /// Build a blocking HTTP client for the given proxy address; an empty address connects directly.
    pub(super) fn build_http_client(proxy: &str) -> color_eyre::Result<Client> {
        let mut builder = Client::builder();
        if !proxy.is_empty() {
            builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(color_eyre::Report::msg)?);
        }
        builder.build().map_err(color_eyre::Report::msg)
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
