use std::path::PathBuf;

use super::EntryType;

/// A destination that already exists while copying, moving or restoring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub source: PathBuf,
    pub target: PathBuf,
    pub source_type: EntryType,
    pub target_type: EntryType,
}

/// How to resolve a [`Conflict`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConflictResolution {
    /// Replace the target (folders are merged).
    Overwrite,
    Skip,
    /// Keep both by giving the new item a unique name.
    Rename,
    /// Cancel the whole operation.
    Cancel,
}

/// The user's answer, optionally applying to every later conflict of the
/// same operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConflictDecision {
    pub resolution: ConflictResolution,
    pub apply_to_all: bool,
}
