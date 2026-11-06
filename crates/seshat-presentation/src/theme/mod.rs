//! Centralised theme system.
//!
//! Components only reference CSS classes. All colours, geometry and
//! typography come from [`Theme`] tokens and are rendered into a single
//! stylesheet by [`render_css`].

mod builtin;
mod color;
mod css;
mod tokens;

pub use builtin::{dark, light};
pub use color::{Color, ColorParseError};
pub use css::render_css;
pub use tokens::{ColorTokens, Geometry, Theme, Typography};

/// Radius of the selected navigation item. Mandated by the product spec, so
/// it is not a theme token and custom themes cannot change it.
pub const NAV_SELECTED_RADIUS_PX: u32 = 12;

/// The user's theme choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePreference {
    /// Resolves the preference to a concrete theme, given whether the system
    /// currently prefers a dark appearance.
    pub fn resolve(self, system_prefers_dark: bool) -> Theme {
        match self {
            Self::Light => light(),
            Self::Dark => dark(),
            Self::System if system_prefers_dark => dark(),
            Self::System => light(),
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::System => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::System,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_preference_follows_system() {
        assert_eq!(ThemePreference::System.resolve(true).name, "Dark");
        assert_eq!(ThemePreference::System.resolve(false).name, "Light");
        assert_eq!(ThemePreference::Light.resolve(true).name, "Light");
    }

    #[test]
    fn preference_cycles_through_all_values() {
        let p = ThemePreference::System;
        assert_eq!(p.next().next().next(), p);
    }
}
