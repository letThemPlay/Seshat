//! File operations: the manager, its requests and events, the filesystem
//! port it runs against, and the operation algorithms.

mod cancel;
#[cfg(any(test, feature = "test-support"))]
pub mod conformance;
mod events;
mod filesystem;
mod job;
mod manager;
mod naming;
mod remove;
mod request;
mod simple;
mod transfer;

#[cfg(test)]
pub(crate) mod memory_fs;
#[cfg(test)]
mod tests;

pub use cancel::CancelToken;
pub use events::{
    EventSubscriber, OperationEvent, OperationEventKind, OperationsSnapshot, SubscriptionId,
};
pub use filesystem::{FileSystem, FsError, FsErrorKind, FsMetadata};
pub use manager::{CancelError, ConflictResolver, OperationManager};
pub use naming::unique_path;
pub(crate) use request::top_level_trash_item;
pub use request::{OperationRequest, RequestError, validate_name};
