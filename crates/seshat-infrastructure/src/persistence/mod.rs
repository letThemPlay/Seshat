//! Configuration files under `$XDG_CONFIG_HOME/seshat`, and the saved
//! workspace under `$XDG_STATE_HOME/seshat`.

mod atomic;
mod preferences;
mod themes;
mod workspace;

pub use preferences::TomlPreferencesStore;
pub use themes::FileThemeRepository;
pub use workspace::TomlWorkspaceStore;

use std::path::PathBuf;

/// `$XDG_CONFIG_HOME/seshat` (usually `~/.config/seshat`).
pub fn config_dir() -> PathBuf {
    glib::user_config_dir().join("seshat")
}
