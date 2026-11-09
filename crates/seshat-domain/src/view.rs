//! How a panel presents its listing: sort order, view mode and filters.
//! Pure value objects shared by panel state and the application view logic.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SortKey {
    #[default]
    Name,
    Size,
    Modified,
    Type,
}

impl SortKey {
    pub const ALL: [Self; 4] = [Self::Name, Self::Size, Self::Modified, Self::Type];

    /// Stable identifier used in settings and UI actions.
    pub fn id(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Size => "size",
            Self::Modified => "modified",
            Self::Type => "type",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Size => "Size",
            Self::Modified => "Modified",
            Self::Type => "Type",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SortOrder {
    pub key: SortKey,
    pub descending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ViewMode {
    #[default]
    List,
    Grid,
}

impl ViewMode {
    pub fn id(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Grid => "grid",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [Self::List, Self::Grid].into_iter().find(|m| m.id() == id)
    }
}

/// Everything that decides which entries are shown and in what order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ViewOptions {
    pub show_hidden: bool,
    pub sort: SortOrder,
    /// Search text; empty shows everything.
    pub query: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for key in SortKey::ALL {
            assert_eq!(SortKey::from_id(key.id()), Some(key));
        }
        for mode in [ViewMode::List, ViewMode::Grid] {
            assert_eq!(ViewMode::from_id(mode.id()), Some(mode));
        }
        assert_eq!(SortKey::from_id("bogus"), None);
    }
}
