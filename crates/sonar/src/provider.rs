use std::sync::Arc;

use crate::error::Result;
use crate::model::{
    PlayUrlResult, ProxyKind, Quality, SearchQuery, SearchResult, SonarSource, Song,
};
use crate::search::SearchConfig;
use async_trait::async_trait;
use reqwest::Client;

use self::bilivideo::BiliVideoProvider;
use self::kugou::KugouProvider;
use self::kuwo::KuwoProvider;
use self::youtube::YoutubeProvider;

// Priorities are declared per source in `SonarSource::default_priority` and applied
// through `SonarProvider::priority`'s default implementation.

/// Build a `reqwest::Client` with the given user agent and an optional HTTP
/// proxy (empty `proxy_url` = direct connection). Fallible so callers can
/// surface an invalid proxy URL or a failed builder instead of panicking.
pub(crate) fn build_client(proxy_url: &str, user_agent: &str) -> Result<Client> {
    let mut builder = Client::builder().user_agent(user_agent);
    if !proxy_url.is_empty() {
        builder = builder.proxy(reqwest::Proxy::all(proxy_url)?);
    }
    Ok(builder.build()?)
}

/// A search backend that resolves songs (and optionally lyrics) from a single
/// third-party source. Implementors are constructed by [`crate::search::SonarFinder`]
/// and queried concurrently; see the trait methods for the per-provider contract.
#[async_trait]
pub trait SonarProvider: Send + Sync {
    /// Which [`SonarSource`] this provider represents.
    fn source(&self) -> SonarSource;

    /// Search the provider for songs matching `query`. Implementations should
    /// return an empty `songs` list (not an error) when nothing matches so the
    /// finder can fall back to other providers.
    async fn search(&self, query: &SearchQuery) -> Result<SearchResult>;

    /// Resolve a playable audio URL for `song` at `quality` (when the provider
    /// honours quality). Returns [`crate::error::SonarError::NoPlayUrl`] when the
    /// song cannot be played (e.g. VIP / copyright restricted).
    async fn get_play_url(&self, song: &Song, quality: Option<Quality>) -> Result<PlayUrlResult>;

    /// Fetch LRC lyrics for a song, if the provider offers them. Returns
    /// `Ok(None)` when no lyrics are available. The default implementation
    /// returns `None`; providers with lyrics override this.
    async fn get_lyrics(&self, song: &Song) -> Result<Option<String>> {
        let _ = song;
        Ok(None)
    }

    /// Whether this provider should be included in the active finder. The
    /// default is `true`; providers can disable themselves (e.g. on a missing
    /// runtime dependency) by returning `false`.
    fn enabled(&self) -> bool {
        true
    }

    /// Search/fallback priority: lower ranks higher, and the finder sorts
    /// providers by priority descending so ties in match score break toward the
    /// preferred source. Defaults to the source's
    /// [`SonarSource::default_priority`]; override only to rank a provider
    /// differently from its source's default.
    fn priority(&self) -> u8 {
        self.source().default_priority()
    }
}

pub mod bilivideo;
pub mod kugou;
pub mod kuwo;
pub mod youtube;

/// Everything needed to construct one provider: adding a source means adding an
/// entry to [`registry`] and listing it in [`SonarSource::ALL`].
pub struct ProviderSpec {
    /// Source this spec builds.
    pub source: SonarSource,
    /// Constructor; receives the session config so it can pick up proxies and
    /// per-source flags (e.g. lossless support).
    pub build: fn(&SearchConfig) -> Result<Arc<dyn SonarProvider>>,
}

fn build_kugou(config: &SearchConfig) -> Result<Arc<dyn SonarProvider>> {
    Ok(Arc::new(KugouProvider::with_proxy(
        config.enable_flac,
        config.proxy_for(ProxyKind::Domestic),
    )?))
}

fn build_kuwo(config: &SearchConfig) -> Result<Arc<dyn SonarProvider>> {
    Ok(Arc::new(KuwoProvider::with_proxy(
        config.proxy_for(ProxyKind::Domestic),
    )?))
}

fn build_bilivideo(config: &SearchConfig) -> Result<Arc<dyn SonarProvider>> {
    Ok(Arc::new(BiliVideoProvider::with_proxy(
        config.proxy_for(ProxyKind::Domestic),
    )?))
}

fn build_youtube(config: &SearchConfig) -> Result<Arc<dyn SonarProvider>> {
    Ok(Arc::new(YoutubeProvider::with_proxy(
        config.proxy_for(ProxyKind::Youtube),
    )?))
}

static REGISTRY: &[ProviderSpec] = &[
    ProviderSpec {
        source: SonarSource::Kugou,
        build: build_kugou,
    },
    ProviderSpec {
        source: SonarSource::Kuwo,
        build: build_kuwo,
    },
    ProviderSpec {
        source: SonarSource::BiliVideo,
        build: build_bilivideo,
    },
    ProviderSpec {
        source: SonarSource::Youtube,
        build: build_youtube,
    },
];

/// The provider registry — the single place where a source becomes available.
///
/// # Adding a source
///
/// 1. add a variant to [`SonarSource`] (name, priority and proxy kind live there),
/// 2. implement [`SonarProvider`] for it,
/// 3. register a [`ProviderSpec`] here and list the source in [`SonarSource::ALL`].
///
/// Nothing else needs to change: configuration and IPC names are parsed with
/// [`SonarSource::from_name`], so the host application picks the new source up
/// as soon as it is listed in `[source_fallback] providers`.
pub fn registry() -> &'static [ProviderSpec] {
    REGISTRY
}

/// Instantiate the providers selected by `config`, skipping providers that
/// report [`SonarProvider::enabled`] as `false`.
///
/// A source that has no registry entry is reported and skipped instead of
/// aborting the session; [`registry`] and [`SonarSource::ALL`] are kept in sync
/// by a unit test.
pub fn build_providers(config: &SearchConfig) -> Result<Vec<Arc<dyn SonarProvider>>> {
    let mut providers = Vec::with_capacity(config.providers.len());

    for source in &config.providers {
        let Some(spec) = registry().iter().find(|spec| spec.source == *source) else {
            log::warn!("sonar: {source} has no registered provider; skipping it");
            continue;
        };

        let provider = (spec.build)(config)?;
        if provider.enabled() {
            providers.push(provider);
        }
    }

    Ok(providers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ensure_crypto() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
    }

    /// Every source must be constructible through the registry: adding a variant
    /// to `SonarSource` without registering a provider would silently disable it.
    #[test]
    fn registry_covers_every_source() {
        for source in SonarSource::ALL {
            assert!(
                registry().iter().any(|spec| spec.source == *source),
                "{source} has no registry entry"
            );
        }
        assert_eq!(
            registry().len(),
            SonarSource::ALL.len(),
            "registry contains duplicate entries"
        );
    }

    /// Configuration/IPC names round-trip through `from_name` (case-insensitive),
    /// and unknown names are rejected so callers can warn instead of silently
    /// dropping a source.
    #[test]
    fn source_names_round_trip() {
        for source in SonarSource::ALL {
            let name = source.as_str();
            assert_eq!(SonarSource::from_name(name), Some(*source));
            assert_eq!(SonarSource::from_name(&name.to_uppercase()), Some(*source));
            assert_eq!(SonarSource::from_name(&format!(" {name} ")), Some(*source));
        }
        assert_eq!(SonarSource::from_name("spotify"), None);
        assert_eq!(SonarSource::from_name(""), None);
    }

    /// Priorities drive fallback ordering, so they must be distinct and `ALL`
    /// must list them highest-priority first.
    #[test]
    fn default_priorities_are_distinct_and_ordered() {
        let priorities: Vec<u8> = SonarSource::ALL
            .iter()
            .map(SonarSource::default_priority)
            .collect();

        let mut unique = priorities.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            priorities.len(),
            "duplicate priorities: {priorities:?}"
        );
        assert!(
            priorities.windows(2).all(|pair| pair[0] < pair[1]),
            "ALL must be ordered highest priority first: {priorities:?}"
        );
    }

    /// `build_providers` honours the configured selection and order, and the
    /// trait's default priority resolves to each source's declared rank.
    #[test]
    fn build_providers_follows_the_configured_selection() {
        ensure_crypto();

        let config =
            SearchConfig::new().with_providers(vec![SonarSource::Kuwo, SonarSource::Youtube]);
        let providers = build_providers(&config).expect("providers build");

        let sources: Vec<SonarSource> = providers.iter().map(|p| p.source()).collect();
        assert_eq!(sources, vec![SonarSource::Kuwo, SonarSource::Youtube]);
        assert!(providers.iter().all(|p| p.enabled()));
        assert_eq!(
            providers[0].priority(),
            SonarSource::Kuwo.default_priority(),
            "priority must default to the source's declared rank"
        );
    }
}
