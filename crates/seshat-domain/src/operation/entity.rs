use std::path::PathBuf;
use std::time::SystemTime;

use super::{OperationId, OperationKind, OperationStatus, Progress};
use crate::DomainError;
use crate::entities::Location;

/// Type of a filesystem object, as seen without following symlinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryType {
    File,
    Directory,
    Symlink,
    Other,
}

/// A single item that could not be processed; the operation continued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFailure {
    pub path: PathBuf,
    pub message: String,
}

/// A file operation and its observable state.
///
/// The lifecycle can only change through the methods below, which enforce
/// [`OperationStatus`] transitions. Cloning yields a consistent snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    id: OperationId,
    kind: OperationKind,
    status: OperationStatus,
    pub sources: Vec<Location>,
    pub destination: Option<PathBuf>,
    pub files: Progress,
    pub bytes: Progress,
    /// The item currently being processed.
    pub current: Option<PathBuf>,
    /// Waiting for the user to resolve a conflict.
    pub awaiting_decision: bool,
    pub failures: Vec<FileFailure>,
    /// Why the operation failed as a whole.
    pub error: Option<String>,
    pub queued_at: SystemTime,
    pub started_at: Option<SystemTime>,
    pub finished_at: Option<SystemTime>,
}

impl Operation {
    pub fn new(
        id: OperationId,
        kind: OperationKind,
        sources: Vec<Location>,
        destination: Option<PathBuf>,
        now: SystemTime,
    ) -> Self {
        Self {
            id,
            kind,
            status: OperationStatus::Queued,
            sources,
            destination,
            files: Progress::indeterminate(0),
            bytes: Progress::indeterminate(0),
            current: None,
            awaiting_decision: false,
            failures: Vec::new(),
            error: None,
            queued_at: now,
            started_at: None,
            finished_at: None,
        }
    }

    pub fn id(&self) -> OperationId {
        self.id
    }

    pub fn kind(&self) -> OperationKind {
        self.kind
    }

    pub fn status(&self) -> OperationStatus {
        self.status
    }

    pub fn is_finished(&self) -> bool {
        self.status.is_terminal()
    }

    /// Time spent running so far (or in total, once finished).
    pub fn elapsed(&self, now: SystemTime) -> Option<std::time::Duration> {
        let start = self.started_at?;
        self.finished_at.unwrap_or(now).duration_since(start).ok()
    }

    fn transition(&mut self, next: OperationStatus) -> Result<(), DomainError> {
        self.status = self.status.transition(next)?;
        Ok(())
    }

    pub fn start(&mut self, now: SystemTime) -> Result<(), DomainError> {
        self.transition(OperationStatus::Started)?;
        self.started_at = Some(now);
        Ok(())
    }

    /// Preparation (scanning) is done; work on files has begun.
    pub fn begin_work(&mut self) -> Result<(), DomainError> {
        self.transition(OperationStatus::InProgress)
    }

    pub fn request_cancel(&mut self) -> Result<(), DomainError> {
        self.transition(OperationStatus::Cancelling)
    }

    pub fn complete(&mut self, now: SystemTime) -> Result<(), DomainError> {
        self.transition(OperationStatus::Completed)?;
        self.finish(now);
        Ok(())
    }

    pub fn fail(&mut self, error: String, now: SystemTime) -> Result<(), DomainError> {
        self.transition(OperationStatus::Failed)?;
        self.error = Some(error);
        self.finish(now);
        Ok(())
    }

    pub fn mark_cancelled(&mut self, now: SystemTime) -> Result<(), DomainError> {
        self.transition(OperationStatus::Cancelled)?;
        self.finish(now);
        Ok(())
    }

    fn finish(&mut self, now: SystemTime) {
        self.finished_at = Some(now);
        self.current = None;
        self.awaiting_decision = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op() -> Operation {
        Operation::new(
            OperationId::new(),
            OperationKind::Copy,
            vec![Location::Path("/a".into())],
            Some("/b".into()),
            SystemTime::UNIX_EPOCH,
        )
    }

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs)
    }

    #[test]
    fn lifecycle_records_times() {
        let mut op = op();
        assert_eq!(op.status(), OperationStatus::Queued);
        op.start(at(1)).unwrap();
        op.begin_work().unwrap();
        assert_eq!(op.elapsed(at(4)), Some(std::time::Duration::from_secs(3)));
        op.complete(at(5)).unwrap();
        assert!(op.is_finished());
        assert_eq!(op.elapsed(at(100)), Some(std::time::Duration::from_secs(4)));
    }

    #[test]
    fn cannot_complete_before_starting() {
        let mut op = op();
        assert!(op.complete(at(1)).is_err());
        assert_eq!(op.status(), OperationStatus::Queued);
    }

    #[test]
    fn cancel_while_queued() {
        let mut op = op();
        op.request_cancel().unwrap();
        op.mark_cancelled(at(1)).unwrap();
        assert_eq!(op.status(), OperationStatus::Cancelled);
    }

    #[test]
    fn failure_keeps_error_and_clears_progress_state() {
        let mut op = op();
        op.start(at(1)).unwrap();
        op.current = Some("/a/x".into());
        op.awaiting_decision = true;
        op.fail("disk full".into(), at(2)).unwrap();
        assert_eq!(op.error.as_deref(), Some("disk full"));
        assert_eq!(op.current, None);
        assert!(!op.awaiting_decision);
        assert!(op.mark_cancelled(at(3)).is_err(), "terminal");
    }
}
