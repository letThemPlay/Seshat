use std::path::{Path, PathBuf};

use seshat_application::ports::StandardLocations;
use seshat_domain::entities::{Location, StandardLocation, StandardLocationKind};

/// Resolves standard locations from the XDG user-dirs configuration via GLib.
///
/// Directory names may be localised; labels come from the actual directory
/// name, never from hardcoded English names.
#[derive(Debug, Default, Clone, Copy)]
pub struct XdgStandardLocations;

impl XdgStandardLocations {
    pub fn new() -> Self {
        Self
    }
}

const USER_DIRS: [(StandardLocationKind, glib::UserDirectory); 6] = [
    (StandardLocationKind::Desktop, glib::UserDirectory::Desktop),
    (
        StandardLocationKind::Documents,
        glib::UserDirectory::Documents,
    ),
    (
        StandardLocationKind::Downloads,
        glib::UserDirectory::Downloads,
    ),
    (
        StandardLocationKind::Pictures,
        glib::UserDirectory::Pictures,
    ),
    (StandardLocationKind::Music, glib::UserDirectory::Music),
    (StandardLocationKind::Videos, glib::UserDirectory::Videos),
];

impl StandardLocations for XdgStandardLocations {
    fn standard_locations(&self) -> Vec<StandardLocation> {
        let home = glib::home_dir();
        let mut locations = vec![StandardLocation {
            kind: StandardLocationKind::Home,
            label: "Home".to_owned(),
            location: Location::Path(home.clone()),
        }];

        for (kind, dir) in USER_DIRS {
            let Some(path) = glib::user_special_dir(dir) else {
                continue;
            };
            if let Some(location) = resolve_user_dir(kind, path, &home) {
                locations.push(location);
            }
        }

        locations.push(StandardLocation {
            kind: StandardLocationKind::Trash,
            label: "Trash".to_owned(),
            location: Location::Trash,
        });
        locations
    }
}

/// xdg-user-dirs marks a disabled directory by pointing it at `$HOME`, and
/// configured directories may not exist. Both are omitted.
fn resolve_user_dir(
    kind: StandardLocationKind,
    path: PathBuf,
    home: &Path,
) -> Option<StandardLocation> {
    if path == home || !path.is_dir() {
        return None;
    }
    let label = glib::filename_display_basename(&path).to_string();
    Some(StandardLocation {
        kind,
        label,
        location: Location::Path(path),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_dir_pointing_at_home_is_skipped() {
        let home = tempfile::tempdir().unwrap();
        let resolved = resolve_user_dir(
            StandardLocationKind::Music,
            home.path().to_path_buf(),
            home.path(),
        );
        assert_eq!(resolved, None);
    }

    #[test]
    fn missing_dir_is_skipped() {
        let home = tempfile::tempdir().unwrap();
        let resolved = resolve_user_dir(
            StandardLocationKind::Videos,
            home.path().join("Vidéos"),
            home.path(),
        );
        assert_eq!(resolved, None);
    }

    #[test]
    fn localised_dir_uses_its_own_name_as_label() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("Téléchargements");
        std::fs::create_dir(&path).unwrap();
        let resolved =
            resolve_user_dir(StandardLocationKind::Downloads, path.clone(), home.path()).unwrap();
        assert_eq!(resolved.label, "Téléchargements");
        assert_eq!(resolved.location, Location::Path(path));
    }

    #[test]
    fn always_includes_home_and_trash() {
        let locations = XdgStandardLocations::new().standard_locations();
        assert_eq!(locations.first().unwrap().kind, StandardLocationKind::Home);
        assert_eq!(locations.last().unwrap().kind, StandardLocationKind::Trash);
    }
}
