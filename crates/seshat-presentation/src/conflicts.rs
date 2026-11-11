//! Bridges conflict questions from worker threads to dialogs on the UI
//! thread.

use std::sync::mpsc;
use std::time::Duration;

use seshat_application::operations::{CancelToken, ConflictResolver};
use seshat_domain::operation::{Conflict, ConflictDecision, ConflictResolution, Operation};

use crate::app::{Msg, OperationsMsg};

/// A conflict waiting for the user, with the channel the worker waits on.
pub struct ConflictRequest {
    pub operation: Operation,
    pub conflict: Conflict,
    pub reply: mpsc::Sender<ConflictDecision>,
}

impl std::fmt::Debug for ConflictRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConflictRequest")
            .field("operation", &self.operation.id())
            .finish_non_exhaustive()
    }
}

/// Asks the UI and blocks the worker until it answers. Cancelling the
/// operation (e.g. from the progress panel) unblocks it immediately.
pub struct UiConflictResolver {
    ui: relm4::Sender<Msg>,
}

impl UiConflictResolver {
    pub fn new(ui: relm4::Sender<Msg>) -> Self {
        Self { ui }
    }
}

const CANCEL: ConflictDecision = ConflictDecision {
    resolution: ConflictResolution::Cancel,
    apply_to_all: false,
};

impl ConflictResolver for UiConflictResolver {
    fn resolve(
        &self,
        operation: &Operation,
        conflict: &Conflict,
        cancel: &CancelToken,
    ) -> ConflictDecision {
        let (reply, answer) = mpsc::channel();
        let request = ConflictRequest {
            operation: operation.clone(),
            conflict: conflict.clone(),
            reply,
        };
        if self
            .ui
            .send(Msg::Operations(OperationsMsg::Conflict(request)))
            .is_err()
        {
            return CANCEL; // UI is gone
        }
        loop {
            match answer.recv_timeout(Duration::from_millis(100)) {
                Ok(decision) => return decision,
                Err(mpsc::RecvTimeoutError::Timeout) if !cancel.is_cancelled() => {}
                Err(_) => return CANCEL,
            }
        }
    }
}
