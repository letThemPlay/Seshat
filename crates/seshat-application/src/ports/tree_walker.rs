use std::ffi::OsString;
use std::path::{Path, PathBuf};

use seshat_domain::entities::EntryKind;
use thiserror::Error;

use crate::operations::CancelToken;

/// Entries a walk visits at most by default, so memory stays bounded.
pub const DEFAULT_MAX_ENTRIES: usize = 200_000;

/// How a folder tree is walked for Fuzzy Find (Spec 4 §6, §12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkOptions {
    /// Include names starting with a dot.
    pub include_hidden: bool,
    /// Names skipped wherever they appear, with everything below them.
    /// Exact names only, for now (decision F3).
    pub ignore_names: Vec<OsString>,
    /// Stay on the root's filesystem: mount points are listed but not
    /// entered.
    pub same_device: bool,
    /// Stop after this many entries (the result is then marked truncated).
    pub max_entries: usize,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            include_hidden: false,
            ignore_names: vec![".git".into(), "node_modules".into()],
            same_device: true,
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }
}

/// One entry found below the root (never the root itself).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub kind: EntryKind,
}

/// What a walk saw besides its candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WalkStats {
    /// Folders that could not be read (permissions, vanished) and were
    /// skipped.
    pub unreadable: usize,
    /// `max_entries` was reached before the walk finished.
    pub truncated: bool,
}

/// Why the root itself could not be walked.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WalkError {
    #[error("“{}” does not exist", .0.display())]
    NotFound(PathBuf),
    #[error("you don't have permission to read “{}”", .0.display())]
    PermissionDenied(PathBuf),
    #[error("“{}” is not a folder", .0.display())]
    NotAFolder(PathBuf),
    #[error("“{}” could not be read: {}", .0.display(), .1)]
    Other(PathBuf, String),
}

/// Walks a folder tree without following symbolic links (Spec 4 §12).
/// Runs on a worker thread; must stop promptly once `cancel` is set.
pub trait TreeWalker: Send + Sync {
    fn walk(
        &self,
        root: &Path,
        options: &WalkOptions,
        cancel: &CancelToken,
        found: &mut dyn FnMut(Candidate),
    ) -> Result<WalkStats, WalkError>;
}
