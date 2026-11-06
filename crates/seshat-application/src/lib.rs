//! Seshat application layer.
//!
//! Defines the ports (traits) infrastructure must implement and the use cases
//! presentation invokes. Presentation never touches the filesystem directly.

pub mod ports;
pub mod use_cases;

use std::future::Future;
use std::pin::Pin;

/// A boxed future that is not required to be `Send`.
///
/// Toolkit-integrated I/O (GIO) runs on the UI thread's main context, so
/// ports do not demand `Send` futures.
pub type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;
