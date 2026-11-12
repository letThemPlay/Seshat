use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use seshat_application::preferences::{
    FuzzyFindPreferences, Preferences, PreferencesStore, ScopeChoice, ThemeChoice, WorkspaceLayout,
    WorkspacePreferences,
};
use seshat_application::view::{SortKey, SortOrder, ViewMode};
use seshat_domain::search::CaseMode;

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
    workspace: Option<WorkspaceSection>,
    fuzzy_find: Option<FuzzyFindSection>,
    /// Action id → key sequences, e.g. `focus-mode = ["zz"]`.
    keybindings: Option<BTreeMap<String, Vec<String>>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct FuzzyFindSection {
    default_scope: Option<String>,
    include_hidden: Option<bool>,
    ignore: Option<Vec<String>>,
    max_results: Option<usize>,
    case: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct WorkspaceSection {
    restore: Option<bool>,
    retain_focus_mode: Option<bool>,
    classic_keys: Option<bool>,
    wrap_cursor: Option<bool>,
    layout: Option<String>,
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
        workspace: workspace_from(file.workspace.unwrap_or_default(), file.keybindings),
        fuzzy_find: fuzzy_find_from(file.fuzzy_find.unwrap_or_default()),
    }
}

fn to_file(preferences: &Preferences) -> SettingsFile {
    SettingsFile {
        theme: Some(preferences.theme.id()),
        show_hidden: Some(preferences.show_hidden),
        view_mode: Some(preferences.view_mode.id().to_owned()),
        sort_key: Some(preferences.sort.key.id().to_owned()),
        sort_descending: Some(preferences.sort.descending),
        workspace: Some(WorkspaceSection {
            restore: Some(preferences.workspace.restore),
            retain_focus_mode: Some(preferences.workspace.retain_focus_mode),
            classic_keys: Some(preferences.workspace.classic_keys),
            wrap_cursor: Some(preferences.workspace.wrap_cursor),
            layout: Some(preferences.workspace.layout.id().to_owned()),
        }),
        fuzzy_find: Some(FuzzyFindSection {
            default_scope: Some(preferences.fuzzy_find.default_scope.id().to_owned()),
            include_hidden: Some(preferences.fuzzy_find.include_hidden),
            ignore: Some(preferences.fuzzy_find.ignore.clone()),
            max_results: Some(preferences.fuzzy_find.max_results),
            case: Some(preferences.fuzzy_find.case.id().to_owned()),
        }),
        keybindings: (!preferences.workspace.key_bindings.is_empty())
            .then(|| preferences.workspace.key_bindings.iter().cloned().collect()),
    }
}

fn workspace_from(
    section: WorkspaceSection,
    keybindings: Option<BTreeMap<String, Vec<String>>>,
) -> WorkspacePreferences {
    let defaults = WorkspacePreferences::default();
    WorkspacePreferences {
        restore: section.restore.unwrap_or(defaults.restore),
        retain_focus_mode: section
            .retain_focus_mode
            .unwrap_or(defaults.retain_focus_mode),
        classic_keys: section.classic_keys.unwrap_or(defaults.classic_keys),
        wrap_cursor: section.wrap_cursor.unwrap_or(defaults.wrap_cursor),
        layout: section
            .layout
            .as_deref()
            .and_then(WorkspaceLayout::from_id)
            .unwrap_or(defaults.layout),
        key_bindings: keybindings.unwrap_or_default().into_iter().collect(),
    }
}

fn fuzzy_find_from(section: FuzzyFindSection) -> FuzzyFindPreferences {
    let defaults = FuzzyFindPreferences::default();
    FuzzyFindPreferences {
        default_scope: section
            .default_scope
            .as_deref()
            .and_then(ScopeChoice::from_id)
            .unwrap_or(defaults.default_scope),
        include_hidden: section.include_hidden.unwrap_or(defaults.include_hidden),
        // Empty names would match nothing; they are dropped.
        ignore: section
            .ignore
            .map(|names| names.into_iter().filter(|n| !n.is_empty()).collect())
            .unwrap_or(defaults.ignore),
        max_results: section
            .max_results
            .filter(|n| FuzzyFindPreferences::MAX_RESULTS.contains(n))
            .unwrap_or(defaults.max_results),
        case: section
            .case
            .as_deref()
            .and_then(CaseMode::from_id)
            .unwrap_or(defaults.case),
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
            workspace: WorkspacePreferences {
                restore: false,
                retain_focus_mode: false,
                classic_keys: true,
                wrap_cursor: true,
                layout: WorkspaceLayout::Strip,
                key_bindings: vec![("focus-mode".into(), vec!["<C-f>".into(), "zf".into()])],
            },
            fuzzy_find: FuzzyFindPreferences {
                default_scope: ScopeChoice::EntireWorkspace,
                include_hidden: true,
                ignore: vec!["target".into(), ".cache".into()],
                max_results: 50,
                case: CaseMode::Smart,
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
    fn invalid_fuzzy_find_values_fall_back_individually() {
        let (_dir, store) = store();
        std::fs::create_dir_all(store.path.parent().unwrap()).unwrap();
        std::fs::write(
            &store.path,
            "[fuzzy_find]\ndefault_scope = \"everywhere\"\ninclude_hidden = true\nignore = [\"target\", \"\"]\nmax_results = 0\ncase = \"shouting\"\n",
        )
        .unwrap();
        let fuzzy = store.load().fuzzy_find;
        let defaults = FuzzyFindPreferences::default();
        assert_eq!(fuzzy.default_scope, defaults.default_scope);
        assert!(fuzzy.include_hidden);
        assert_eq!(fuzzy.ignore, ["target"], "empty names are dropped");
        assert_eq!(fuzzy.max_results, defaults.max_results, "0 is out of range");
        assert_eq!(fuzzy.case, defaults.case);

        std::fs::write(
            &store.path,
            "[fuzzy_find]\nignore = []\nmax_results = 10000\n",
        )
        .unwrap();
        let fuzzy = store.load().fuzzy_find;
        assert!(fuzzy.ignore.is_empty(), "an empty list ignores nothing");
        assert_eq!(fuzzy.max_results, 10_000);
    }

    #[test]
    fn corrupt_file_gives_defaults() {
        let (_dir, store) = store();
        std::fs::create_dir_all(store.path.parent().unwrap()).unwrap();
        std::fs::write(&store.path, "this is = = not toml").unwrap();
        assert_eq!(store.load(), Preferences::default());
    }

    /// docs/examples/settings.toml must stay complete and correct: it
    /// lists every configurable action with its default binding, and its
    /// other values are the defaults too.
    #[test]
    fn the_example_settings_list_every_binding_with_its_default() {
        use seshat_application::interaction::{Action, KeyMap};

        let (_dir, store) = store();
        std::fs::create_dir_all(store.path.parent().unwrap()).unwrap();
        std::fs::write(
            &store.path,
            include_str!("../../../../docs/examples/settings.toml"),
        )
        .unwrap();
        let prefs = store.load();

        let listed: Vec<&str> = prefs
            .workspace
            .key_bindings
            .iter()
            .map(|(id, _)| id.as_str())
            .collect();
        for action in Action::ALL {
            assert!(listed.contains(&action.id()), "missing {}", action.id());
        }
        assert_eq!(listed.len(), Action::ALL.len(), "unknown actions listed");

        let (example, warnings) = KeyMap::new(&prefs.workspace.key_bindings, false);
        assert!(warnings.is_empty(), "{warnings:?}");
        let defaults = KeyMap::default();
        for action in Action::ALL {
            assert_eq!(
                example.keys_for(action),
                defaults.keys_for(action),
                "{} differs from its default",
                action.id()
            );
        }

        // Apart from the bindings, it is exactly the defaults.
        let without_bindings = Preferences {
            workspace: WorkspacePreferences {
                key_bindings: Vec::new(),
                ..prefs.workspace.clone()
            },
            ..prefs
        };
        assert_eq!(without_bindings, Preferences::default());
    }
}
