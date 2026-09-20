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
