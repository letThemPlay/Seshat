use gio::prelude::*;
use seshat_application::ports::{DirectoryChange, DirectoryWatcher, ListingError, WatchGuard};
use seshat_domain::entities::Location;

use super::directory_reader::listing_error;
use super::gio_file::gio_file;

/// Watches directories with `GFileMonitor` (inotify locally, GVfs for Trash).
#[derive(Debug, Default, Clone, Copy)]
pub struct GioDirectoryWatcher;

impl GioDirectoryWatcher {
    pub fn new() -> Self {
        Self
    }
}

/// Cancels the monitor when the watch is dropped.
struct MonitorGuard(gio::FileMonitor);

impl Drop for MonitorGuard {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

impl DirectoryWatcher for GioDirectoryWatcher {
    fn watch(
        &self,
        location: &Location,
        on_change: Box<dyn Fn(DirectoryChange)>,
    ) -> Result<WatchGuard, ListingError> {
        let dir = gio_file(location);
        let monitor = dir
            .monitor_directory(
                gio::FileMonitorFlags::WATCH_MOVES | gio::FileMonitorFlags::WATCH_MOUNTS,
                gio::Cancellable::NONE,
            )
            .map_err(|err| listing_error(&err))?;

        monitor.connect_changed(move |_, file, _other, event| {
            if let Some(change) = classify(file.equal(&dir), event) {
                on_change(change);
            }
        });
        Ok(WatchGuard::new(MonitorGuard(monitor)))
    }
}

fn classify(is_watched_dir: bool, event: gio::FileMonitorEvent) -> Option<DirectoryChange> {
    use gio::FileMonitorEvent as E;
    match event {
        E::Deleted | E::MovedOut | E::Unmounted if is_watched_dir => Some(DirectoryChange::Removed),
        E::Created
        | E::Deleted
        | E::MovedIn
        | E::MovedOut
        | E::Renamed
        | E::AttributeChanged
        | E::ChangesDoneHint => Some(DirectoryChange::Modified),
        // `Changed` fires repeatedly while a file is written; wait for
        // `ChangesDoneHint` instead.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gio::FileMonitorEvent as E;

    #[test]
    fn classification() {
        assert_eq!(classify(true, E::Deleted), Some(DirectoryChange::Removed));
        assert_eq!(classify(true, E::Unmounted), Some(DirectoryChange::Removed));
        assert_eq!(classify(false, E::Deleted), Some(DirectoryChange::Modified));
        assert_eq!(classify(false, E::Created), Some(DirectoryChange::Modified));
        assert_eq!(classify(false, E::Changed), None);
        assert_eq!(classify(false, E::PreUnmount), None);
    }
}
