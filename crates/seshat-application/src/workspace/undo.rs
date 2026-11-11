//! Workspace undo: structural workspace changes that can be reversed.
//!
//! This undoes changes to the workspace (panels and branches), never file
//! operations. Only backward pruning records changes so far (ADR 0007,
//! decision B5). The history lasts for the session and is not saved.

use thiserror::Error;

use seshat_domain::workspace::{PanelId, PruneRefusal, PrunedSegment, RestoreRefusal};

/// Workspace changes kept for undo at most.
pub const UNDO_LIMIT: usize = 20;

/// A reversible workspace change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceChange {
    Prune(PrunedSegment),
}

/// The most recent reversible workspace changes, newest last.
#[derive(Debug, Default)]
pub(super) struct WorkspaceUndo {
    changes: Vec<WorkspaceChange>,
}

impl WorkspaceUndo {
    pub(super) fn push(&mut self, change: WorkspaceChange) {
        self.changes.push(change);
        if self.changes.len() > UNDO_LIMIT {
            self.changes.remove(0);
        }
    }

    pub(super) fn pop(&mut self) -> Option<WorkspaceChange> {
        self.changes.pop()
    }

    pub(super) fn len(&self) -> usize {
        self.changes.len()
    }
}

/// Why a prune did not happen. Nothing was changed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PruneError {
    /// Pruning is turned off in Focus Mode (decision B9).
    #[error("pruning isn't available in Focus Mode")]
    FocusMode,
    #[error(transparent)]
    Refused(#[from] PruneRefusal),
}

/// Why undo did not happen. Nothing was changed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UndoError {
    /// Undo is turned off in Focus Mode (decision B9).
    #[error("undo isn't available in Focus Mode")]
    FocusMode,
    #[error("there is no workspace change to undo")]
    NothingToUndo,
    /// The change can no longer be undone; it was dropped from the history.
    #[error("can't undo: {0}")]
    Refused(#[from] RestoreRefusal),
}

/// What a prune did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PruneOutcome {
    /// The panel that is now active.
    pub destination: PanelId,
    /// Panels removed from the workspace.
    pub removed: usize,
    /// Selected items that were in the removed panels.
    pub deselected: usize,
    /// The pruned branch had no panels of its own left and ended.
    pub ended_branch: bool,
}

/// What undo did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UndoOutcome {
    /// Panels put back.
    pub restored: usize,
}
