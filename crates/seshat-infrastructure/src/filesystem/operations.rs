//! Blocking filesystem primitives for file operations (runs on worker
//! threads).

use std::ffi::OsString;
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use gio::prelude::*;
use seshat_application::operations::{FileSystem, FsError, FsErrorKind, FsMetadata};
use seshat_domain::operation::EntryType;

use super::gio_file::gio_file;
use seshat_domain::entities::Location;

/// Mode bits kept on copies. setuid/setgid are dropped (like `cp` when
/// ownership changes): a copy must never gain privileges. Folders keep the
/// sticky bit.
const COPIED_FILE_BITS: u32 = 0o777;
const COPIED_DIR_BITS: u32 = 0o1777;

/// Copy granularity: progress and cancellation are checked between chunks.
/// Within a chunk the kernel copies directly (`copy_file_range`).
const CHUNK: u64 = 8 * 1024 * 1024;

/// `std::fs` + `rustix` for local files, GIO for the Trash.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalFileSystem;

impl LocalFileSystem {
    pub fn new() -> Self {
        Self
    }
}

fn io_error(err: &io::Error) -> FsError {
    use io::ErrorKind as K;
    let (kind, message) = match err.kind() {
        K::NotFound => (
            FsErrorKind::NotFound,
            "The item no longer exists.".to_owned(),
        ),
        K::PermissionDenied => (
            FsErrorKind::PermissionDenied,
            "Permission denied.".to_owned(),
        ),
        K::AlreadyExists => (
            FsErrorKind::AlreadyExists,
            "An item with this name already exists.".to_owned(),
        ),
        K::StorageFull | K::QuotaExceeded => (
            FsErrorKind::NoSpace,
            "No space left on the device.".to_owned(),
        ),
        K::CrossesDevices => (
            FsErrorKind::CrossDevice,
            "Source and target are on different devices.".to_owned(),
        ),
        K::ReadOnlyFilesystem => (
            FsErrorKind::ReadOnly,
            "The location is read-only.".to_owned(),
        ),
        K::DirectoryNotEmpty => (FsErrorKind::NotEmpty, "The folder is not empty.".to_owned()),
        K::Unsupported => (FsErrorKind::NotSupported, "Not supported here.".to_owned()),
        _ => (FsErrorKind::Other, err.to_string()),
    };
    FsError::new(kind, message)
}

fn rustix_error(err: rustix::io::Errno) -> FsError {
    io_error(&io::Error::from(err))
}

fn gio_error(err: &glib::Error) -> FsError {
    let kind = match err.kind::<gio::IOErrorEnum>() {
        Some(gio::IOErrorEnum::NotFound) => FsErrorKind::NotFound,
        Some(gio::IOErrorEnum::PermissionDenied) => FsErrorKind::PermissionDenied,
        Some(gio::IOErrorEnum::Exists) => FsErrorKind::AlreadyExists,
        Some(gio::IOErrorEnum::NoSpace) => FsErrorKind::NoSpace,
        Some(gio::IOErrorEnum::NotSupported) => FsErrorKind::NotSupported,
        Some(gio::IOErrorEnum::ReadOnly) => FsErrorKind::ReadOnly,
        Some(gio::IOErrorEnum::Cancelled) => FsErrorKind::Cancelled,
        _ => FsErrorKind::Other,
    };
    FsError::new(kind, err.message())
}

fn trash_file(item: &Path) -> gio::File {
    gio_file(&Location::Trash(item.to_path_buf()))
}

impl FileSystem for LocalFileSystem {
    fn metadata(&self, path: &Path) -> Result<FsMetadata, FsError> {
        let meta = fs::symlink_metadata(path).map_err(|e| io_error(&e))?;
        let file_type = meta.file_type();
        let entry_type = if file_type.is_symlink() {
            EntryType::Symlink
        } else if file_type.is_dir() {
            EntryType::Directory
        } else if file_type.is_file() {
            EntryType::File
        } else {
            EntryType::Other
        };
        Ok(FsMetadata {
            entry_type,
            size: if entry_type == EntryType::File {
                meta.len()
            } else {
                0
            },
            device: meta.dev(),
        })
    }

    /// Refuses to list through a symlink (`O_NOFOLLOW`). Recursive operations
    /// check an entry with `lstat` and then list it; if it was swapped for a
    /// symlink in between (e.g. in a shared folder like /tmp), following it
    /// would make a delete empty the link's target.
    fn read_dir(&self, path: &Path) -> Result<Vec<OsString>, FsError> {
        use rustix::fs::{Dir, Mode, OFlags};
        let fd = rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|errno| match errno {
            rustix::io::Errno::LOOP | rustix::io::Errno::NOTDIR => FsError::new(
                FsErrorKind::Other,
                "Not a folder (it may have been replaced by a link).",
            ),
            other => rustix_error(other),
        })?;
        let mut names = Vec::new();
        for entry in Dir::read_from(&fd).map_err(rustix_error)? {
            let entry = entry.map_err(rustix_error)?;
            let name = entry.file_name().to_bytes();
            if name != b"." && name != b".." {
                names.push(
                    <std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(name)
                        .to_os_string(),
                );
            }
        }
        Ok(names)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        fs::canonicalize(path).map_err(|e| io_error(&e))
    }

    fn create_dir(&self, path: &Path) -> Result<(), FsError> {
        fs::create_dir(path).map_err(|e| io_error(&e))
    }

    fn create_file(&self, path: &Path) -> Result<(), FsError> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map(drop)
            .map_err(|e| io_error(&e))
    }

    fn copy_file(
        &self,
        from: &Path,
        to: &Path,
        on_progress: &mut dyn FnMut(u64) -> bool,
    ) -> Result<(), FsError> {
        let source = File::open(from).map_err(|e| io_error(&e))?;
        let meta = source.metadata().map_err(|e| io_error(&e))?;
        // Owner-only until complete, then the source's permissions.
        let target = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(to)
            .map_err(|e| io_error(&e))?;
        let mut writer = &target;
        loop {
            let copied =
                io::copy(&mut (&source).take(CHUNK), &mut writer).map_err(|e| io_error(&e))?;
            if copied == 0 {
                break;
            }
            if !on_progress(copied) {
                return Err(FsError::new(FsErrorKind::Cancelled, "Cancelled."));
            }
        }
        target
            .set_permissions(fs::Permissions::from_mode(meta.mode() & COPIED_FILE_BITS))
            .map_err(|e| io_error(&e))?;
        if let Ok(modified) = meta.modified() {
            let _ = target.set_times(FileTimes::new().set_modified(modified));
        }
        Ok(())
    }

    fn copy_symlink(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        let target = fs::read_link(from).map_err(|e| io_error(&e))?;
        std::os::unix::fs::symlink(target, to).map_err(|e| io_error(&e))
    }

    fn copy_dir_attributes(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        let meta = fs::metadata(from).map_err(|e| io_error(&e))?;
        fs::set_permissions(
            to,
            fs::Permissions::from_mode(meta.mode() & COPIED_DIR_BITS),
        )
        .map_err(|e| io_error(&e))?;
        if let Ok(modified) = meta.modified()
            && let Ok(dir) = File::open(to)
        {
            let _ = dir.set_times(FileTimes::new().set_modified(modified));
        }
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path, replace: bool) -> Result<(), FsError> {
        if replace {
            return fs::rename(from, to).map_err(|e| io_error(&e));
        }
        // Atomic "rename unless the target exists".
        match rustix::fs::renameat_with(
            rustix::fs::CWD,
            from,
            rustix::fs::CWD,
            to,
            rustix::fs::RenameFlags::NOREPLACE,
        ) {
            Ok(()) => Ok(()),
            // Filesystem without RENAME_NOREPLACE: check, then rename.
            Err(rustix::io::Errno::INVAL | rustix::io::Errno::NOSYS) => {
                if fs::symlink_metadata(to).is_ok() {
                    return Err(FsError::new(
                        FsErrorKind::AlreadyExists,
                        "An item with this name already exists.",
                    ));
                }
                fs::rename(from, to).map_err(|e| io_error(&e))
            }
            Err(errno) => Err(rustix_error(errno)),
        }
    }

    fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        fs::remove_file(path).map_err(|e| io_error(&e))
    }

    fn remove_empty_dir(&self, path: &Path) -> Result<(), FsError> {
        fs::remove_dir(path).map_err(|e| io_error(&e))
    }

    fn available_space(&self, path: &Path) -> Option<u64> {
        let stat = rustix::fs::statvfs(path).ok()?;
        Some(stat.f_bavail.saturating_mul(stat.f_frsize))
    }

    fn trash(&self, path: &Path) -> Result<(), FsError> {
        gio::File::for_path(path)
            .trash(gio::Cancellable::NONE)
            .map_err(|e| gio_error(&e))
    }

    fn trash_origin(&self, item: &Path) -> Result<PathBuf, FsError> {
        let info = trash_file(item)
            .query_info(
                "trash::orig-path",
                gio::FileQueryInfoFlags::NONE,
                gio::Cancellable::NONE,
            )
            .map_err(|e| gio_error(&e))?;
        info.attribute_byte_string("trash::orig-path")
            .map(|s| PathBuf::from(s.as_str()))
            .ok_or_else(|| FsError::new(FsErrorKind::NotFound, "Original location unknown."))
    }

    fn restore(&self, item: &Path, to: &Path, replace: bool) -> Result<(), FsError> {
        let mut flags = gio::FileCopyFlags::NOFOLLOW_SYMLINKS | gio::FileCopyFlags::ALL_METADATA;
        if replace {
            flags |= gio::FileCopyFlags::OVERWRITE;
        }
        trash_file(item)
            .move_(
                &gio::File::for_path(to),
                flags,
                gio::Cancellable::NONE,
                None,
            )
            .map_err(|e| gio_error(&e))
    }

    fn delete_trashed(&self, item: &Path) -> Result<(), FsError> {
        trash_file(item)
            .delete(gio::Cancellable::NONE)
            .map_err(|e| gio_error(&e))
    }
}
