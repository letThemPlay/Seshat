use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use gio::prelude::*;
use seshat_application::LocalBoxFuture;
use seshat_application::ports::{DirectoryReader, ListingError};
use seshat_domain::entities::{EntryKind, FileEntry, Location};

/// Only metadata is queried; file contents are never read.
const ATTRIBUTES: &str = "standard::name,standard::type,standard::size,\
                          standard::is-hidden,standard::is-symlink,time::modified";

/// Entries are fetched in batches so large directories don't need one huge
/// allocation from GIO and the main loop stays responsive.
const BATCH_SIZE: i32 = 256;

const TRASH_URI: &str = "trash:///";

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
            let dir = match location {
                Location::Path(path) => gio::File::for_path(path),
                Location::Trash => gio::File::for_uri(TRASH_URI),
            };
            enumerate(&dir).await.map_err(|err| {
                let mapped = map_error(&err);
                tracing::debug!(uri = %dir.uri(), error = %err, "directory listing failed");
                mapped
            })
        })
    }
}

async fn enumerate(dir: &gio::File) -> Result<Vec<FileEntry>, glib::Error> {
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
        entries.extend(
            batch
                .iter()
                .map(|info| to_entry(&enumerator.child(info), info)),
        );
    }
    Ok(entries)
}

fn to_entry(child: &gio::File, info: &gio::FileInfo) -> FileEntry {
    let kind = if info.is_symlink() {
        EntryKind::Symlink
    } else {
        match info.file_type() {
            gio::FileType::Directory => EntryKind::Directory,
            gio::FileType::Regular => EntryKind::File,
            _ => EntryKind::Other,
        }
    };

    FileEntry {
        name: info.name().into_os_string(),
        // Virtual GVfs locations (e.g. Trash) have no local path. Phase 2
        // models Trash entries properly; until then the URI identifies them.
        path: child
            .path()
            .unwrap_or_else(|| PathBuf::from(child.uri().as_str())),
        kind,
        size: (kind != EntryKind::Directory)
            .then(|| u64::try_from(info.size()).ok())
            .flatten(),
        modified: info.modification_date_time().and_then(|dt| {
            let secs = u64::try_from(dt.to_unix()).ok()?;
            let micros = u64::try_from(dt.microsecond()).ok()?;
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs) + Duration::from_micros(micros))
        }),
        hidden: info.is_hidden(),
    }
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
