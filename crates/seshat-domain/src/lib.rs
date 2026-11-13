//! Seshat domain layer.
//!
//! Holds entities, value objects and invariants. This crate must not depend on
//! UI toolkits, D-Bus, async runtimes or concrete filesystem implementations
//! (checked by `scripts/check-architecture.sh`).

pub mod direction;
pub mod entities;
pub mod error;
pub mod operation;
pub mod search;
mod stable_ids;
pub mod view;
pub mod workspace;

pub use direction::Direction;
pub use error::DomainError;
