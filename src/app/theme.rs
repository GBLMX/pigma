use super::App;
use crate::{
    config::{Config, Theme, ThemeRegistry, theme_fallback},
    utils::terminal::{BACKGROUND, Background},
};

impl App {
    /// Resolve the current theme from the terminal's background: `light_theme` on a light
    /// background, otherwise `default_theme`. Falls back to `default` if the configured
    /// name is missing, then to a hardcoded fallback.
    /// Borrows individual fields rather than the whole `&self` so callers can use
    /// it without holding an overall borrow.
    pub(crate) fn resolve_theme<'a>(config: &Config, registry: &'a ThemeRegistry) -> &'a Theme {
        let wanted = match config.background.resolve(*BACKGROUND) {
            Background::Light => config
                .light_theme
                .as_deref()
                .unwrap_or(&config.default_theme),
            Background::Dark => &config.default_theme,
        };

        registry.get(wanted).unwrap_or_else(|| {
            log::warn!("Theme '{wanted}' not found, falling back to default");
            registry.get("default").unwrap_or_else(|| {
                log::error!("Default theme missing, using hardcoded fallback");
                theme_fallback()
            })
        })
    }

    pub fn current_theme(&self) -> &Theme {
        Self::resolve_theme(&self.config, &self.theme_registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RANDOM_THEME;

    /// A config that asks for `random` must be settled before anything draws. `resolve_theme`
    /// runs on every frame and `random` is not a theme, so a name left unresolved would log a
    /// warning on each frame and quietly show `default` instead.
    #[tokio::test]
    async fn a_random_theme_is_settled_before_the_first_frame() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let config = Config {
            default_theme: RANDOM_THEME.to_string(),
            ..Config::default()
        };

        let app = App::new(config, false).expect("app");

        assert_ne!(app.config.default_theme, RANDOM_THEME);
        assert!(
            app.theme_registry.get(&app.config.default_theme).is_some(),
            "`{}` does not resolve",
            app.config.default_theme
        );
        assert_eq!(app.current_theme().name, app.config.default_theme);
    }

    /// The light slot rolls separately: with `background = auto` both slots can be `random`.
    #[tokio::test]
    async fn both_slots_settle_independently() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let config = Config {
            default_theme: RANDOM_THEME.to_string(),
            light_theme: Some(RANDOM_THEME.to_string()),
            ..Config::default()
        };

        let app = App::new(config, false).expect("app");

        let light = app.config.light_theme.as_deref().expect("light slot");
        assert_ne!(light, RANDOM_THEME);
        assert!(app.theme_registry.get(light).is_some(), "`{light}`");
    }
}
