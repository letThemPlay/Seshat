use seshat_domain::entities::Location;

use super::ListingError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryChange {
    /// Entries were added, removed or changed.
    Modified,
    /// The watched directory itself was deleted, moved or unmounted.
    Removed,
}

/// Keeps a watch alive; dropping it stops watching.
pub struct WatchGuard {
    _inner: Box<dyn std::any::Any>,
}

impl WatchGuard {
    pub fn new<T: 'static>(inner: T) -> Self {
        Self {
            _inner: Box::new(inner),
        }
    }
}

impl std::fmt::Debug for WatchGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WatchGuard")
    }
}

/// Notifies about changes to a directory's contents.
pub trait DirectoryWatcher {
    fn watch(
        &self,
        location: &Location,
        on_change: Box<dyn Fn(DirectoryChange)>,
    ) -> Result<WatchGuard, ListingError>;
}
