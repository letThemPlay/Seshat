//! File operation model: identity, lifecycle and progress.

mod id;
mod progress;
mod status;

pub use id::OperationId;
pub use progress::Progress;
pub use status::OperationStatus;
