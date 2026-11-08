use thiserror::Error;

use crate::operation::OperationStatus;

/// Violations of domain invariants.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    #[error("invalid operation transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: OperationStatus,
        to: OperationStatus,
    },

    #[error("completed amount {completed} exceeds total {total}")]
    ProgressExceedsTotal { completed: u64, total: u64 },
}
