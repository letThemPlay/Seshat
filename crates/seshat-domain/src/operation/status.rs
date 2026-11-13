use crate::DomainError;

/// Lifecycle state of a file operation.
///
/// ```text
/// Queued -> Started -> InProgress -> Completed | Failed | Cancelling
/// Started -> Failed
/// Cancelling -> Cancelled | Completed | Failed
/// ```
///
/// Cancellation may also be requested before work has begun (`Queued` or
/// `Started`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationStatus {
    Queued,
    Started,
    InProgress,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
}

impl OperationStatus {
    pub const ALL: [Self; 7] = [
        Self::Queued,
        Self::Started,
        Self::InProgress,
        Self::Cancelling,
        Self::Completed,
        Self::Failed,
        Self::Cancelled,
    ];

    /// Terminal states never transition again.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        use OperationStatus::*;
        matches!(
            (self, next),
            (Queued, Started)
                | (Queued, Cancelling)
                | (Started, InProgress)
                | (Started, Failed)
                | (Started, Cancelling)
                | (InProgress, Completed)
                | (InProgress, Failed)
                | (InProgress, Cancelling)
                | (Cancelling, Cancelled)
                | (Cancelling, Completed)
                | (Cancelling, Failed)
        )
    }

    /// Returns the next state, or an error if the transition is not allowed.
    pub fn transition(self, next: Self) -> Result<Self, DomainError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(DomainError::InvalidTransition {
                from: self,
                to: next,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OperationStatus::{self, *};
    use crate::DomainError;

    const VALID: &[(OperationStatus, OperationStatus)] = &[
        (Queued, Started),
        (Queued, Cancelling),
        (Started, InProgress),
        (Started, Failed),
        (Started, Cancelling),
        (InProgress, Completed),
        (InProgress, Failed),
        (InProgress, Cancelling),
        (Cancelling, Cancelled),
        (Cancelling, Completed),
        (Cancelling, Failed),
    ];

    #[test]
    fn every_pair_matches_the_lifecycle_table() {
        for from in OperationStatus::ALL {
            for to in OperationStatus::ALL {
                let expected = VALID.contains(&(from, to));
                assert_eq!(from.can_transition_to(to), expected, "{from:?} -> {to:?}");
            }
        }
    }

    #[test]
    fn terminal_states_have_no_outgoing_transitions() {
        for from in OperationStatus::ALL.into_iter().filter(|s| s.is_terminal()) {
            for to in OperationStatus::ALL {
                assert!(!from.can_transition_to(to), "{from:?} -> {to:?}");
            }
        }
    }

    #[test]
    fn invalid_transition_is_an_error() {
        assert_eq!(
            Completed.transition(InProgress),
            Err(DomainError::InvalidTransition {
                from: Completed,
                to: InProgress
            })
        );
    }

    #[test]
    fn happy_path_walks_the_lifecycle() {
        let status = Queued
            .transition(Started)
            .and_then(|s| s.transition(InProgress))
            .and_then(|s| s.transition(Completed));
        assert_eq!(status, Ok(Completed));
    }

    #[test]
    fn no_self_transitions() {
        for s in OperationStatus::ALL {
            assert!(!s.can_transition_to(s));
        }
    }
}
