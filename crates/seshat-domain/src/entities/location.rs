use std::path::{Path, PathBuf};

/// Something the explorer can list or address.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Location {
    /// A local (or locally mounted) filesystem path.
    Path(PathBuf),
    /// A location inside the freedesktop.org Trash, relative to its root.
    /// The Trash spans several filesystems, so it is not a single directory
    /// on disk. An empty path is the Trash root.
    Trash(PathBuf),
}

impl Location {
    pub fn trash_root() -> Self {
        Self::Trash(PathBuf::new())
    }

    pub fn is_trash(&self) -> bool {
        matches!(self, Self::Trash(_))
    }

    /// The containing location, or `None` at a filesystem or Trash root.
    pub fn parent(&self) -> Option<Self> {
        match self {
            Self::Path(path) => path.parent().map(|p| Self::Path(p.to_path_buf())),
            Self::Trash(rel) if rel.as_os_str().is_empty() => None,
            Self::Trash(rel) => Some(Self::Trash(
                rel.parent().map(Path::to_path_buf).unwrap_or_default(),
            )),
        }
    }

    /// The location of a direct child named `name`.
    pub fn child(&self, name: &std::ffi::OsStr) -> Self {
        match self {
            Self::Path(path) => Self::Path(path.join(name)),
            Self::Trash(rel) => Self::Trash(rel.join(name)),
        }
    }

    /// Whether this location is `root` or lies beneath it.
    pub fn is_within(&self, root: &Path) -> bool {
        matches!(self, Self::Path(path) if path.starts_with(root))
    }
}

/// Well-known user locations shown in the navigation pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StandardLocationKind {
    Home,
    Desktop,
    Documents,
    Downloads,
    Pictures,
    Music,
    Videos,
    Trash,
}

/// A resolved standard location.
///
/// `label` comes from the platform (it may be localised), not from the kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandardLocation {
    pub kind: StandardLocationKind,
    pub label: String,
    pub location: Location,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn path_parent() {
        let loc = Location::Path("/home/a/b".into());
        assert_eq!(loc.parent(), Some(Location::Path("/home/a".into())));
        assert_eq!(Location::Path("/".into()).parent(), None);
    }

    #[test]
    fn trash_parent_stops_at_root() {
        let nested = Location::Trash("dir/sub".into());
        assert_eq!(nested.parent(), Some(Location::Trash("dir".into())));
        assert_eq!(
            Location::Trash("dir".into()).parent(),
            Some(Location::trash_root())
        );
        assert_eq!(Location::trash_root().parent(), None);
    }

    #[test]
    fn child_joins_within_the_same_kind() {
        assert_eq!(
            Location::trash_root().child(OsStr::new("x")),
            Location::Trash("x".into())
        );
        assert_eq!(
            Location::Path("/a".into()).child(OsStr::new("b")),
            Location::Path("/a/b".into())
        );
    }

    #[test]
    fn is_within_matches_whole_components() {
        let loc = Location::Path("/run/media/usb/docs".into());
        assert!(loc.is_within(Path::new("/run/media/usb")));
        assert!(!loc.is_within(Path::new("/run/media/us")));
        assert!(!Location::trash_root().is_within(Path::new("/")));
    }
}
