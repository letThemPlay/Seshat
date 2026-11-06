//! Seshat domain layer.
//!
//! Holds entities, value objects and invariants. This crate must not depend on
//! UI toolkits, D-Bus, async runtimes or concrete filesystem implementations.

pub mod entities;
pub mod error;
pub mod operation;

pub use error::DomainError;
