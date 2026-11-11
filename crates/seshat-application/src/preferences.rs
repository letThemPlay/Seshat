//! User preferences persisted between sessions.

use seshat_domain::view::ViewOptions;
use seshat_domain::workspace::PanelState;

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

impl Preferences {
    /// The settings a new root panel starts with (decision D11).
    pub fn panel_defaults(&self) -> PanelState {
        PanelState {
            options: ViewOptions {
                show_hidden: self.show_hidden,
                sort: self.sort,
                query: String::new(),
            },
            mode: self.view_mode,
            ..PanelState::default()
        }
    }

    /// Remembers `defaults` as the settings for new panels, after a panel
    /// setting changed (decision D11). A panel's search is never saved.
    pub fn adopt_panel_defaults(&mut self, defaults: &PanelState) {
        self.show_hidden = defaults.options.show_hidden;
        self.view_mode = defaults.mode;
        self.sort = defaults.options.sort;
    }
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
    /// Branches side by side (`Tree`) or only the active one (`Strip`).
    pub layout: WorkspaceLayout,
    /// Key binding overrides: action id and its key sequences.
    pub key_bindings: Vec<(String, Vec<String>)>,
}

seshat_domain::stable_ids! {
    /// How branches are laid out (decision L2).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum WorkspaceLayout {
        /// Branches side by side: tree depth across, branches down, with
        /// connectors. Falls back to one row when the window is too short.
        #[default]
        Tree => "tree",
        /// Only the active branch, as a single strip.
        Strip => "strip",
    }
}

impl Default for WorkspacePreferences {
    fn default() -> Self {
        Self {
            restore: true,
            retain_focus_mode: true,
            classic_keys: false,
            wrap_cursor: false,
            layout: WorkspaceLayout::Tree,
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

    #[test]
    fn panel_defaults_round_trip_without_the_search() {
        let mut prefs = Preferences::default();
        let mut state = prefs.panel_defaults();
        state.options.show_hidden = true;
        state.options.sort.descending = true;
        state.options.query = "draft".into();
        state.mode = ViewMode::Grid;
        prefs.adopt_panel_defaults(&state);
        assert!(prefs.show_hidden);
        assert!(prefs.sort.descending);
        assert_eq!(prefs.view_mode, ViewMode::Grid);
        let back = prefs.panel_defaults();
        assert_eq!(back.options.query, "", "a search is not a default");
        assert_eq!(back.mode, ViewMode::Grid);
        assert_eq!(back.options.sort, state.options.sort);
    }
}
