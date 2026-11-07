//! File operation model: identity, kind, lifecycle, progress and conflicts.

mod conflict;
mod entity;
mod id;
mod kind;
mod progress;
mod status;

pub use conflict::{Conflict, ConflictDecision, ConflictResolution};
pub use entity::{EntryType, FileFailure, Operation};
pub use id::OperationId;
pub use kind::OperationKind;
pub use progress::Progress;
pub use status::OperationStatus;
