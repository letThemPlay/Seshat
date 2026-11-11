//! How a panel presents its listing: sort order, view mode and filters.
//! Pure value objects shared by panel state and the application view logic.

crate::stable_ids! {
    /// What a panel is sorted by.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub enum SortKey {
        #[default]
        Name => "name",
        Size => "size",
        Modified => "modified",
        Type => "type",
    }
}

impl SortKey {
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

crate::stable_ids! {
    /// How a panel shows its entries.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub enum ViewMode {
        #[default]
        List => "list",
        Grid => "grid",
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
