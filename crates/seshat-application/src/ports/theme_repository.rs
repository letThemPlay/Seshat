use std::path::PathBuf;

/// The raw source of a custom theme file. Parsing and validation belong to
/// the theme system, so the repository only reads files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeSource {
    /// File stem; stable identifier of the theme.
    pub id: String,
    pub path: PathBuf,
    /// File contents, or why they could not be read.
    pub contents: Result<String, String>,
}

/// Discovers user-defined themes.
pub trait ThemeRepository {
    /// Directory users put theme files in (may not exist yet).
    fn directory(&self) -> PathBuf;

    /// Creates the directory if needed (so it can be watched and opened).
    fn ensure_directory(&self) -> Result<PathBuf, String>;

    fn load_all(&self) -> Vec<ThemeSource>;
}
