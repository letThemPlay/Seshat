use std::path::PathBuf;

/// Something the explorer can list.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Location {
    /// A local (or locally mounted) directory.
    Path(PathBuf),
    /// The freedesktop.org Trash, which is not a single directory on disk.
    Trash,
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
