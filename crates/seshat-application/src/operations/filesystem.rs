use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

use seshat_domain::operation::EntryType;

/// Metadata of a path, read without following symlinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsMetadata {
    pub entry_type: EntryType,
    /// Size in bytes (regular files; 0 otherwise).
    pub size: u64,
    /// Device the entry lives on; equal devices allow cheap renames.
    pub device: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsErrorKind {
    NotFound,
    PermissionDenied,
    AlreadyExists,
    NoSpace,
    /// The operation needs source and target on the same filesystem.
    CrossDevice,
    ReadOnly,
    NotEmpty,
    NotSupported,
    /// Stopped because the operation was cancelled.
    Cancelled,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsError {
    pub kind: FsErrorKind,
    pub message: String,
}

impl FsError {
    pub fn new(kind: FsErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FsError {}

/// Blocking filesystem primitives used by file operations. Called from
/// operation worker threads, never from the UI thread.
///
/// Implementations must never follow symlinks implicitly and must never
/// fall back to deletion when moving to the Trash fails.
pub trait FileSystem: Send + Sync {
    fn metadata(&self, path: &Path) -> Result<FsMetadata, FsError>;

    fn exists(&self, path: &Path) -> bool {
        self.metadata(path).is_ok()
    }

    /// Names of the directory's entries (unsorted).
    fn read_dir(&self, path: &Path) -> Result<Vec<OsString>, FsError>;

    /// Creates one directory; fails with `AlreadyExists` if present.
    fn create_dir(&self, path: &Path) -> Result<(), FsError>;

    /// Creates an empty file; fails with `AlreadyExists` if present.
    fn create_file(&self, path: &Path) -> Result<(), FsError>;

    /// Copies a regular file's contents, permissions and modification time
    /// to `to`, which must not exist. `on_progress` receives byte deltas and
    /// returns `false` to cancel (the error is then `Cancelled`). Callers
    /// clean up a partial `to` on error.
    fn copy_file(
        &self,
        from: &Path,
        to: &Path,
        on_progress: &mut dyn FnMut(u64) -> bool,
    ) -> Result<(), FsError>;

    /// Recreates the symlink `from` at `to` (the link, not its target).
    fn copy_symlink(&self, from: &Path, to: &Path) -> Result<(), FsError>;

    /// Copies permissions and modification time of a directory.
    fn copy_dir_attributes(&self, from: &Path, to: &Path) -> Result<(), FsError>;

    /// Renames within one filesystem. Without `replace`, fails with
    /// `AlreadyExists` instead of replacing an existing target.
    fn rename(&self, from: &Path, to: &Path, replace: bool) -> Result<(), FsError>;

    /// Removes a file or symlink.
    fn remove_file(&self, path: &Path) -> Result<(), FsError>;

    fn remove_empty_dir(&self, path: &Path) -> Result<(), FsError>;

    /// Free bytes available to the user on the filesystem of `path`.
    fn available_space(&self, path: &Path) -> Option<u64>;

    /// Moves `path` to the Trash. Must fail (not delete) if unsupported.
    fn trash(&self, path: &Path) -> Result<(), FsError>;

    /// Original location of a top-level Trash item.
    fn trash_origin(&self, item: &Path) -> Result<PathBuf, FsError>;

    /// Moves a Trash item back to `to`.
    fn restore(&self, item: &Path, to: &Path, replace: bool) -> Result<(), FsError>;

    /// Permanently deletes a Trash item (recursively).
    fn delete_trashed(&self, item: &Path) -> Result<(), FsError>;
}
