//! Configuration files under `$XDG_CONFIG_HOME/seshat`.

mod atomic;
mod preferences;
mod themes;

pub use preferences::TomlPreferencesStore;
pub use themes::FileThemeRepository;

use std::path::PathBuf;

/// `$XDG_CONFIG_HOME/seshat` (usually `~/.config/seshat`).
pub fn config_dir() -> PathBuf {
    glib::user_config_dir().join("seshat")
}
