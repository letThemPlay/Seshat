use std::path::PathBuf;

use seshat_application::ports::{ThemeRepository, ThemeSource};

/// Largest theme file we read; themes are small and this bounds memory.
const MAX_THEME_BYTES: u64 = 64 * 1024;

/// Reads `*.toml` files from the themes directory, and the theme file given
/// on the command line.
#[derive(Debug, Clone)]
pub struct FileThemeRepository {
    directory: PathBuf,
    provided: Option<PathBuf>,
}

impl FileThemeRepository {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            provided: None,
        }
    }

    /// `$XDG_CONFIG_HOME/seshat/themes`.
    pub fn default_location() -> Self {
        Self::new(super::config_dir().join("themes"))
    }

    /// Also reads `path`: the theme file given with `--theme-file`.
    pub fn with_provided(mut self, path: PathBuf) -> Self {
        self.provided = Some(path);
        self
    }
}

impl ThemeRepository for FileThemeRepository {
    fn directory(&self) -> PathBuf {
        self.directory.clone()
    }

    fn ensure_directory(&self) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&self.directory).map_err(|e| e.to_string())?;
        Ok(self.directory.clone())
    }

    fn load_all(&self) -> Vec<ThemeSource> {
        let Ok(dir) = std::fs::read_dir(&self.directory) else {
            return Vec::new(); // missing or unreadable themes directory
        };
        let mut sources: Vec<ThemeSource> = dir
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
            .filter_map(|path| {
                let id = path.file_stem()?.to_str()?.to_owned();
                Some(ThemeSource {
                    contents: read_limited(&path),
                    id,
                    path,
                })
            })
            .collect();
        sources.sort_by(|a, b| a.id.cmp(&b.id));
        sources
    }

    fn load_provided(&self) -> Option<ThemeSource> {
        let path = self.provided.clone()?;
        // Only used in messages and as the name of a theme without one.
        let id = path.file_stem().map_or_else(
            || "provided".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        Some(ThemeSource {
            contents: read_limited(&path),
            id,
            path,
        })
    }
}

fn read_limited(path: &std::path::Path) -> Result<String, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_THEME_BYTES {
        return Err(format!(
            "file is larger than {} KiB",
            MAX_THEME_BYTES / 1024
        ));
    }
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_directory_has_no_themes() {
        let dir = tempfile::tempdir().unwrap();
        let repo = FileThemeRepository::new(dir.path().join("themes"));
        assert!(repo.load_all().is_empty());
    }

    #[test]
    fn loads_only_toml_files_sorted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("zen.toml"), "name = \"Zen\"").unwrap();
        std::fs::write(dir.path().join("aurora.toml"), "name = \"Aurora\"").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "ignored").unwrap();
        let sources = FileThemeRepository::new(dir.path().to_path_buf()).load_all();
        let ids: Vec<_> = sources.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["aurora", "zen"]);
        assert_eq!(sources[0].contents.as_deref(), Ok("name = \"Aurora\""));
    }

    #[test]
    fn the_provided_file_is_read_apart_from_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("stylix.toml");
        std::fs::write(&file, "name = \"Stylix\"").unwrap();
        let repo = FileThemeRepository::new(dir.path().join("themes"));
        assert_eq!(repo.load_provided(), None, "no --theme-file");

        let repo = repo.with_provided(file.clone());
        assert!(repo.load_all().is_empty(), "not one of the folder's themes");
        let provided = repo.load_provided().unwrap();
        assert_eq!(provided.id, "stylix");
        assert_eq!(provided.path, file);
        assert_eq!(provided.contents.as_deref(), Ok("name = \"Stylix\""));
    }

    #[test]
    fn a_missing_provided_file_reports_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let repo = FileThemeRepository::new(dir.path().to_path_buf())
            .with_provided(dir.path().join("gone.toml"));
        assert!(repo.load_provided().unwrap().contents.is_err());
    }

    #[test]
    fn oversized_and_invalid_utf8_files_report_errors() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("big.toml"), vec![b'#'; 70 * 1024]).unwrap();
        std::fs::write(dir.path().join("bin.toml"), [0xff, 0xfe]).unwrap();
        let sources = FileThemeRepository::new(dir.path().to_path_buf()).load_all();
        assert!(sources.iter().all(|s| s.contents.is_err()));
    }
}
