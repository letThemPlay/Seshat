//! D-Bus IPC: exposes the File Operation Manager on the session bus so
//! other applications (panels, status bars) can observe and cancel file
//! operations. See `docs/ipc/protocol.md` for the contract.
//!
//! This is a thin adapter: all state lives in the
//! [`OperationManager`](seshat_application::operations::OperationManager).

mod service;
mod types;

pub use service::{IpcConfig, IpcError, IpcService};
pub use types::OperationInfo;

/// Protocol version reported by `GetProtocolVersion`. Bumped on any
/// incompatible change to methods, signals or `OperationInfo`.
pub const PROTOCOL_VERSION: u32 = 1;

/// Well-known bus name. Placeholder until the project identity is decided;
/// distinct from the GTK application id, which GApplication owns.
pub const BUS_NAME: &str = "com.example.Seshat.Operations";
pub const OBJECT_PATH: &str = "/com/example/Seshat/Operations";
pub const INTERFACE: &str = "com.example.Seshat.Operations";
