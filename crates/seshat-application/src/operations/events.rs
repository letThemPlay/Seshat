use seshat_domain::operation::Operation;

/// What happened to an operation. Mirrors the D-Bus signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationEventKind {
    Queued,
    Started,
    /// Progress or other non-lifecycle change (throttled).
    Progress,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
}

/// An event with the operation's state right after the change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationEvent {
    /// Strictly increasing across all operations. Consumers detect gaps
    /// and resynchronise with a snapshot.
    pub sequence: u64,
    pub kind: OperationEventKind,
    pub operation: Operation,
}

/// Active operations as of `sequence`: every event with a higher sequence
/// happened after this snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsSnapshot {
    pub sequence: u64,
    pub operations: Vec<Operation>,
}

/// Identifies a subscription for [`super::OperationManager::unsubscribe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(pub(crate) u64);

/// Event callback. Called on worker threads with internal locks held so
/// that delivery order equals sequence order: it must be quick and must not
/// call back into the manager (forward to a channel instead).
pub type EventSubscriber = Box<dyn Fn(&OperationEvent) + Send + Sync>;
