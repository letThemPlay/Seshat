//! Centralised theme system.
//!
//! Components only reference CSS classes. All colours, geometry and
//! typography come from [`Theme`] tokens and are rendered into a single
//! stylesheet by [`render_css`].

mod builtin;
mod color;
mod css;
mod custom;
mod tokens;

pub use builtin::{dark, light};
pub use color::{Color, ColorParseError};
pub use css::render_css;
pub use custom::{CustomTheme, contrast_ratio, parse_custom_theme};
pub use tokens::{ColorTokens, Geometry, Theme, Typography};

/// Radius of the selected navigation item. Mandated by the product spec, so
/// it is not a theme token and custom themes cannot change it.
pub const NAV_SELECTED_RADIUS_PX: u32 = 12;

use seshat_application::ports::ThemeSource;
use seshat_application::preferences::ThemeChoice;

/// A custom theme file that could not be used at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeLoadError {
    pub id: String,
    pub message: String,
}

/// Built-in plus successfully parsed custom themes.
#[derive(Debug, Clone, Default)]
pub struct ThemeRegistry {
    customs: Vec<CustomTheme>,
    errors: Vec<ThemeLoadError>,
}

impl ThemeRegistry {
    /// Parses theme sources. Problems are logged and kept for the UI; a
    /// broken file never prevents other themes from loading.
    pub fn from_sources(sources: Vec<ThemeSource>) -> Self {
        let mut registry = Self::default();
        for source in sources {
            let parsed = source
                .contents
                .and_then(|text| parse_custom_theme(&source.id, &text));
            match parsed {
                Ok(theme) => {
                    for warning in &theme.warnings {
                        tracing::warn!(theme = %theme.id, path = %source.path.display(), "{warning}");
                    }
                    registry.customs.push(theme);
                }
                Err(message) => {
                    tracing::warn!(theme = %source.id, path = %source.path.display(), error = %message, "theme not loaded");
                    registry.errors.push(ThemeLoadError {
                        id: source.id,
                        message,
                    });
                }
            }
        }
        registry
    }

    pub fn customs(&self) -> &[CustomTheme] {
        &self.customs
    }

    pub fn errors(&self) -> &[ThemeLoadError] {
        &self.errors
    }

    pub fn custom(&self, id: &str) -> Option<&CustomTheme> {
        self.customs.iter().find(|t| t.id == id)
    }

    /// Resolves a choice to a concrete theme. `None` if a custom theme is
    /// chosen but not available (deleted or invalid).
    pub fn resolve(&self, choice: &ThemeChoice, system_prefers_dark: bool) -> Option<Theme> {
        match choice {
            ThemeChoice::System if system_prefers_dark => Some(dark()),
            ThemeChoice::System | ThemeChoice::Light => Some(light()),
            ThemeChoice::Dark => Some(dark()),
            ThemeChoice::Custom(id) => self.custom(id).map(|t| t.theme.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn source(id: &str, contents: Result<&str, &str>) -> ThemeSource {
        ThemeSource {
            id: id.to_owned(),
            path: PathBuf::from(format!("/themes/{id}.toml")),
            contents: contents.map(str::to_owned).map_err(str::to_owned),
        }
    }

    #[test]
    fn system_follows_system_preference() {
        let registry = ThemeRegistry::default();
        assert!(registry.resolve(&ThemeChoice::System, true).unwrap().dark);
        assert!(!registry.resolve(&ThemeChoice::System, false).unwrap().dark);
        assert!(!registry.resolve(&ThemeChoice::Light, true).unwrap().dark);
    }

    #[test]
    fn broken_files_do_not_block_others() {
        let registry = ThemeRegistry::from_sources(vec![
            source("good", Ok("name = \"Good\"\nextends = \"dark\"")),
            source("syntax", Ok("name = ")),
            source("unreadable", Err("permission denied")),
        ]);
        assert_eq!(registry.customs().len(), 1);
        assert_eq!(registry.errors().len(), 2);
        let good = registry
            .resolve(&ThemeChoice::Custom("good".into()), false)
            .unwrap();
        assert_eq!(good.name, "Good");
        assert!(good.dark);
        assert_eq!(
            registry.resolve(&ThemeChoice::Custom("syntax".into()), false),
            None
        );
    }
}
