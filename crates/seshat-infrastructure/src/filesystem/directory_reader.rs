use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use gio::prelude::*;
use seshat_application::LocalBoxFuture;
use seshat_application::ports::{DirectoryReader, ListingError};
use seshat_domain::entities::{EntryKind, FileEntry, Location, TrashInfo};

use super::gio_file::gio_file;

/// Only metadata is queried; file contents are never read.
const ATTRIBUTES: &str = "standard::name,standard::type,standard::size,\
                          standard::is-hidden,standard::display-name,standard::fast-content-type,standard::is-symlink,time::modified,\
                          trash::orig-path,trash::deletion-date";

/// Entries are fetched in batches so large directories don't need one huge
/// allocation from GIO and the main loop stays responsive.
const BATCH_SIZE: i32 = 256;

/// Lists directories through GIO's async enumerator.
///
/// Uses GIO rather than `std::fs` so that GVfs locations (Trash, network
/// mounts) work with the same code path.
#[derive(Debug, Default, Clone, Copy)]
pub struct GioDirectoryReader;

impl GioDirectoryReader {
    pub fn new() -> Self {
        Self
    }
}

impl DirectoryReader for GioDirectoryReader {
    fn read_directory<'a>(
        &'a self,
        location: &'a Location,
    ) -> LocalBoxFuture<'a, Result<Vec<FileEntry>, ListingError>> {
        Box::pin(async move {
            let dir = gio_file(location);
            enumerate(&dir, location).await.map_err(|err| {
                tracing::debug!(uri = %dir.uri(), error = %err, "directory listing failed");
                map_error(&err)
            })
        })
    }
}

async fn enumerate(dir: &gio::File, location: &Location) -> Result<Vec<FileEntry>, glib::Error> {
    let enumerator = dir
        .enumerate_children_future(
            ATTRIBUTES,
            // Report symlinks as links instead of following them.
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            glib::Priority::DEFAULT,
        )
        .await?;

    let mut entries = Vec::new();
    loop {
        let batch = enumerator
            .next_files_future(BATCH_SIZE, glib::Priority::DEFAULT)
            .await?;
        if batch.is_empty() {
            break;
        }
        entries.extend(batch.iter().map(|info| to_entry(location, info)));
    }
    resolve_symlink_targets(dir, &mut entries).await;
    Ok(entries)
}

fn to_entry(parent: &Location, info: &gio::FileInfo) -> FileEntry {
    let kind = if info.is_symlink() {
        // Resolved after the batch; see `resolve_symlink_targets`.
        EntryKind::Symlink {
            to_directory: false,
        }
    } else {
        match info.file_type() {
            gio::FileType::Directory => EntryKind::Directory,
            gio::FileType::Regular => EntryKind::File,
            _ => EntryKind::Other,
        }
    };
    let name = info.name().into_os_string();
    let display_name = match parent {
        Location::Trash(_) => info.display_name().to_string(),
        Location::Path(_) => FileEntry::lossy_name(&name),
    };

    FileEntry {
        location: parent.child(&name),
        name,
        display_name,
        kind,
        // A symlink's own size is the length of its target path, which is
        // meaningless to users; only regular files and specials report one.
        size: matches!(kind, EntryKind::File | EntryKind::Other)
            .then(|| u64::try_from(info.size()).ok())
            .flatten(),
        modified: info
            .modification_date_time()
            .and_then(|dt| system_time(&dt)),
        hidden: info.is_hidden(),
        content_type: info
            .attribute_string("standard::fast-content-type")
            .map(|s| s.to_string()),
        trash_info: trash_info(info),
    }
}

fn trash_info(info: &gio::FileInfo) -> Option<TrashInfo> {
    // orig-path is a filename-encoded byte string; GIO hands it back as
    // bytes, so non-UTF-8 paths survive.
    let original = info.attribute_byte_string("trash::orig-path")?;
    Some(TrashInfo {
        original_path: PathBuf::from(original.as_str()),
        deleted_at: info.deletion_date().and_then(|dt| system_time(&dt)),
    })
}

fn system_time(dt: &glib::DateTime) -> Option<SystemTime> {
    let secs = u64::try_from(dt.to_unix()).ok()?;
    let micros = u64::try_from(dt.microsecond()).ok()?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs) + Duration::from_micros(micros))
}

fn map_error(err: &glib::Error) -> ListingError {
    match err.kind::<gio::IOErrorEnum>() {
        Some(gio::IOErrorEnum::NotFound) => ListingError::NotFound,
        Some(gio::IOErrorEnum::PermissionDenied) => ListingError::PermissionDenied,
        Some(gio::IOErrorEnum::NotDirectory) => ListingError::NotADirectory,
        Some(
            gio::IOErrorEnum::NotMounted
            | gio::IOErrorEnum::NotSupported
            | gio::IOErrorEnum::HostNotFound
            | gio::IOErrorEnum::HostUnreachable
            | gio::IOErrorEnum::TimedOut,
        ) => ListingError::Unavailable(err.message().to_owned()),
        _ => ListingError::Other(err.message().to_owned()),
    }
}

pub(crate) fn listing_error(err: &glib::Error) -> ListingError {
    map_error(err)
}

/// Stats the target of each symlink (one extra query per link, none for
/// regular entries) so links to directories can be browsed. Broken links and
/// unreadable targets stay non-browsable.
async fn resolve_symlink_targets(dir: &gio::File, entries: &mut [FileEntry]) {
    for entry in entries
        .iter_mut()
        .filter(|e| matches!(e.kind, EntryKind::Symlink { .. }))
    {
        let target = dir
            .child(&entry.name)
            .query_info_future(
                "standard::type",
                gio::FileQueryInfoFlags::NONE,
                glib::Priority::DEFAULT,
            )
            .await;
        if let Ok(info) = target {
            entry.kind = EntryKind::Symlink {
                to_directory: info.file_type() == gio::FileType::Directory,
            };
        }
    }
}
