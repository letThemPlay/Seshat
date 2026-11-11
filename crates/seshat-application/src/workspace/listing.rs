use seshat_domain::entities::{FileEntry, Location};
use seshat_domain::workspace::Unavailability;

use crate::ports::ListingError;

/// What an unavailable panel shows instead of its folder (decision D14).
impl From<Unavailability> for ListingError {
    fn from(reason: Unavailability) -> Self {
        match reason {
            Unavailability::Missing => Self::NotFound,
            Unavailability::DeviceRemoved => Self::Unavailable("The device was removed.".into()),
        }
    }
}

/// What a panel's content area shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Loading,
    Ready,
    Empty,
    /// The panel's search filter matches nothing.
    NoResults,
    Failed(ListingError),
}

/// A panel's cached directory listing. Owned by the workspace service, not
/// by any widget, so panels can be re-rendered without re-reading the disk.
#[derive(Debug, Clone)]
pub struct PanelListing {
    /// The full listing; `shown` is derived from it with
    /// [`crate::view::arrange`], so sorting and filtering never touch the disk.
    pub(super) entries: Vec<FileEntry>,
    pub(super) shown: Vec<FileEntry>,
    pub(super) content: Content,
    /// Increases whenever `shown` changes, so renderers know when to update.
    pub(super) revision: u64,
    pub(super) load_generation: u64,
    pub(super) watch_generation: u64,
    pub(super) last_used: u64,
    /// Select this entry once it shows up (after create or rename).
    pub(super) pending_select: Option<Location>,
    /// A listing has arrived at least once.
    pub(super) loaded: bool,
}

impl PanelListing {
    pub(super) fn new(watch_generation: u64, last_used: u64) -> Self {
        Self {
            entries: Vec::new(),
            shown: Vec::new(),
            content: Content::Loading,
            revision: 0,
            load_generation: 0,
            watch_generation,
            last_used,
            pending_select: None,
            loaded: false,
        }
    }

    /// The entries to display, in display order.
    pub fn shown(&self) -> &[FileEntry] {
        &self.shown
    }

    /// Every entry of the folder, before filtering and sorting.
    pub fn entries(&self) -> &[FileEntry] {
        &self.entries
    }

    /// Number of entries in the folder, before filtering.
    pub fn total(&self) -> usize {
        self.entries.len()
    }

    pub fn content(&self) -> &Content {
        &self.content
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn watch_generation(&self) -> u64 {
        self.watch_generation
    }

    pub fn position_of(&self, location: &Location) -> Option<usize> {
        self.shown.iter().position(|e| e.location == *location)
    }

    pub fn entry(&self, location: &Location) -> Option<&FileEntry> {
        self.shown.iter().find(|e| e.location == *location)
    }
}
