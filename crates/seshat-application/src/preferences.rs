//! User preferences persisted between sessions.

use crate::view::{SortOrder, ViewMode};

/// Which theme to use: follow the system, a built-in theme, or a custom
/// theme identified by its file stem.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
    Custom(String),
}

impl ThemeChoice {
    /// Stable identifier for settings and UI actions.
    pub fn id(&self) -> String {
        match self {
            Self::System => "system".to_owned(),
            Self::Light => "light".to_owned(),
            Self::Dark => "dark".to_owned(),
            Self::Custom(name) => format!("custom:{name}"),
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => id
                .strip_prefix("custom:")
                .filter(|name| !name.is_empty())
                .map(|name| Self::Custom(name.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Preferences {
    pub theme: ThemeChoice,
    /// Defaults for new panels (each panel keeps its own settings).
    pub show_hidden: bool,
    pub view_mode: ViewMode,
    pub sort: SortOrder,
    pub workspace: WorkspacePreferences,
}

/// Preferences of the spatial workspace and its keyboard interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePreferences {
    /// Restore the previous workspace on launch (otherwise start at Home).
    pub restore: bool,
    /// Keep Focus Mode when switching branches.
    pub retain_focus_mode: bool,
    /// "Classic keys" (decision D1): typing searches, and single-key
    /// bindings such as `h` or `j` are off.
    pub classic_keys: bool,
    /// Moving past the last item wraps to the first, and vice versa.
    pub wrap_cursor: bool,
    /// Key binding overrides: action id and its key sequences.
    pub key_bindings: Vec<(String, Vec<String>)>,
}

impl Default for WorkspacePreferences {
    fn default() -> Self {
        Self {
            restore: true,
            retain_focus_mode: true,
            classic_keys: false,
            wrap_cursor: false,
            key_bindings: Vec::new(),
        }
    }
}

/// Loads and saves preferences.
pub trait PreferencesStore {
    /// Returns stored preferences, falling back to defaults for anything
    /// missing or invalid. Never fails: a broken settings file must not stop
    /// the application from starting.
    fn load(&self) -> Preferences;

    fn save(&self, preferences: &Preferences) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_ids_round_trip() {
        for choice in [
            ThemeChoice::System,
            ThemeChoice::Light,
            ThemeChoice::Dark,
            ThemeChoice::Custom("nord".into()),
        ] {
            assert_eq!(ThemeChoice::from_id(&choice.id()), Some(choice));
        }
        assert_eq!(ThemeChoice::from_id("custom:"), None);
        assert_eq!(ThemeChoice::from_id("neon"), None);
    }
}
