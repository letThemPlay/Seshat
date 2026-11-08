use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use seshat_application::preferences::{Preferences, PreferencesStore, ThemeChoice};
use seshat_application::view::{SortKey, SortOrder, ViewMode};

use super::atomic::write_atomic;

/// Stores preferences as TOML (`settings.toml`).
#[derive(Debug, Clone)]
pub struct TomlPreferencesStore {
    path: PathBuf,
}

impl TomlPreferencesStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// `$XDG_CONFIG_HOME/seshat/settings.toml`.
    pub fn default_location() -> Self {
        Self::new(super::config_dir().join("settings.toml"))
    }
}

/// On-disk format. Every field is optional so older or hand-edited files
/// still load; unknown or invalid values fall back to defaults.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SettingsFile {
    theme: Option<String>,
    show_hidden: Option<bool>,
    view_mode: Option<String>,
    sort_key: Option<String>,
    sort_descending: Option<bool>,
}

impl PreferencesStore for TomlPreferencesStore {
    fn load(&self) -> Preferences {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Preferences::default();
            }
            Err(err) => {
                tracing::warn!(path = %self.path.display(), error = %err, "cannot read settings; using defaults");
                return Preferences::default();
            }
        };
        let file: SettingsFile = match toml::from_str(&text) {
            Ok(file) => file,
            Err(err) => {
                tracing::warn!(path = %self.path.display(), error = %err, "invalid settings; using defaults");
                return Preferences::default();
            }
        };
        from_file(file)
    }

    fn save(&self, preferences: &Preferences) -> Result<(), String> {
        let text = toml::to_string(&to_file(preferences)).map_err(|e| e.to_string())?;
        write_atomic(&self.path, &text).map_err(|e| {
            tracing::warn!(path = %self.path.display(), error = %e, "cannot save settings");
            e.to_string()
        })
    }
}

fn from_file(file: SettingsFile) -> Preferences {
    let defaults = Preferences::default();
    Preferences {
        theme: file
            .theme
            .as_deref()
            .and_then(ThemeChoice::from_id)
            .unwrap_or(defaults.theme),
        show_hidden: file.show_hidden.unwrap_or(defaults.show_hidden),
        view_mode: file
            .view_mode
            .as_deref()
            .and_then(ViewMode::from_id)
            .unwrap_or(defaults.view_mode),
        sort: SortOrder {
            key: file
                .sort_key
                .as_deref()
                .and_then(SortKey::from_id)
                .unwrap_or(defaults.sort.key),
            descending: file.sort_descending.unwrap_or(defaults.sort.descending),
        },
    }
}

fn to_file(preferences: &Preferences) -> SettingsFile {
    SettingsFile {
        theme: Some(preferences.theme.id()),
        show_hidden: Some(preferences.show_hidden),
        view_mode: Some(preferences.view_mode.id().to_owned()),
        sort_key: Some(preferences.sort.key.id().to_owned()),
        sort_descending: Some(preferences.sort.descending),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, TomlPreferencesStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = TomlPreferencesStore::new(dir.path().join("nested/settings.toml"));
        (dir, store)
    }

    #[test]
    fn missing_file_gives_defaults() {
        let (_dir, store) = store();
        assert_eq!(store.load(), Preferences::default());
    }

    #[test]
    fn round_trip_creates_parent_dirs() {
        let (_dir, store) = store();
        let prefs = Preferences {
            theme: ThemeChoice::Custom("nord".into()),
            show_hidden: true,
            view_mode: ViewMode::Grid,
            sort: SortOrder {
                key: SortKey::Modified,
                descending: true,
            },
        };
        store.save(&prefs).unwrap();
        assert_eq!(store.load(), prefs);
    }

    #[test]
    fn invalid_values_fall_back_individually() {
        let (_dir, store) = store();
        std::fs::create_dir_all(store.path.parent().unwrap()).unwrap();
        std::fs::write(
            &store.path,
            "theme = \"neon\"\nshow_hidden = true\nview_mode = \"cards\"\nsort_key = \"size\"\nunknown = 1\n",
        )
        .unwrap();
        let prefs = store.load();
        assert_eq!(prefs.theme, ThemeChoice::System);
        assert!(prefs.show_hidden);
        assert_eq!(prefs.view_mode, ViewMode::List);
        assert_eq!(prefs.sort.key, SortKey::Size);
    }

    #[test]
    fn corrupt_file_gives_defaults() {
        let (_dir, store) = store();
        std::fs::create_dir_all(store.path.parent().unwrap()).unwrap();
        std::fs::write(&store.path, "this is = = not toml").unwrap();
        assert_eq!(store.load(), Preferences::default());
    }
}
