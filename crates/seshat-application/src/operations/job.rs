//! Execution context of one running operation.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use seshat_domain::operation::{Conflict, ConflictResolution, FileFailure, OperationId, Progress};

use super::cancel::CancelToken;
use super::filesystem::{FileSystem, FsError, FsErrorKind};
use super::manager::Inner;
use super::request::OperationRequest;
use super::{remove, simple, transfer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JobError {
    Cancelled,
    /// The operation as a whole cannot continue.
    Fatal(String),
}

impl From<FsError> for JobError {
    fn from(err: FsError) -> Self {
        match err.kind {
            FsErrorKind::Cancelled => Self::Cancelled,
            _ => Self::Fatal(err.message),
        }
    }
}

pub(crate) type JobResult = Result<(), JobError>;

pub(crate) struct Job<'a> {
    inner: &'a Inner,
    id: OperationId,
    cancel: CancelToken,
    pub(crate) fs: Arc<dyn FileSystem>,
    /// "Apply to all" answer for later conflicts.
    remembered: Option<ConflictResolution>,
    succeeded: usize,
    failed: usize,
    /// Unique token for temporary file names.
    pub(crate) token: String,
}

impl<'a> Job<'a> {
    pub(crate) fn new(inner: &'a Arc<Inner>, id: OperationId, cancel: CancelToken) -> Self {
        Self {
            inner,
            id,
            cancel,
            fs: inner.fs.clone(),
            remembered: None,
            succeeded: 0,
            failed: 0,
            token: id.to_string()[..8].to_owned(),
        }
    }

    pub(crate) fn cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub(crate) fn check_cancel(&self) -> JobResult {
        if self.cancelled() {
            Err(JobError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Records the totals found by scanning and moves to `InProgress`.
    pub(crate) fn begin(&self, files: u64, bytes: Option<u64>) {
        self.inner.update(self.id, true, |op| {
            op.files = Progress::Determinate {
                completed: 0,
                total: files,
            };
            op.bytes = match bytes {
                Some(total) => Progress::Determinate {
                    completed: 0,
                    total,
                },
                None => Progress::indeterminate(0),
            };
            let _ = op.begin_work();
        });
    }

    pub(crate) fn set_current(&self, path: &Path) {
        let path = path.to_path_buf();
        self.inner
            .update(self.id, false, |op| op.current = Some(path));
    }

    pub(crate) fn add_bytes(&self, bytes: u64) {
        self.inner
            .update(self.id, false, |op| op.bytes = op.bytes.advance(bytes));
    }

    /// One item finished successfully.
    pub(crate) fn item_done(&mut self) {
        self.succeeded += 1;
        self.inner
            .update(self.id, false, |op| op.files = op.files.advance(1));
    }

    /// Items skipped by the user still count as processed.
    pub(crate) fn skipped(&self, files: u64, bytes: u64) {
        self.inner.update(self.id, false, |op| {
            op.files = op.files.advance(files);
            op.bytes = op.bytes.advance(bytes);
        });
    }

    /// One item failed; the operation continues with the rest.
    pub(crate) fn item_failed(&mut self, path: &Path, message: impl Into<String>) {
        self.failed += 1;
        let failure = FileFailure {
            path: path.to_path_buf(),
            message: message.into(),
        };
        tracing::warn!(operation = %self.id, path = %failure.path.display(), error = %failure.message, "item failed");
        self.inner.update(self.id, true, |op| {
            op.files = op.files.advance(1);
            op.failures.push(failure);
        });
    }

    /// Asks how to resolve a conflict, honouring an earlier "apply to all".
    pub(crate) fn resolve(&mut self, conflict: Conflict) -> Result<ConflictResolution, JobError> {
        if let Some(resolution) = self.remembered {
            return Ok(resolution);
        }
        self.inner
            .update(self.id, true, |op| op.awaiting_decision = true);
        let Some(snapshot) = self.inner.snapshot_of(self.id) else {
            return Err(JobError::Cancelled);
        };
        let decision = self
            .inner
            .resolver()
            .resolve(&snapshot, &conflict, &self.cancel);
        self.inner
            .update(self.id, true, |op| op.awaiting_decision = false);

        self.check_cancel()?;
        if decision.resolution == ConflictResolution::Cancel {
            self.cancel.cancel();
            return Err(JobError::Cancelled);
        }
        if decision.apply_to_all {
            self.remembered = Some(decision.resolution);
        }
        Ok(decision.resolution)
    }

    /// Final result: a run where every item failed is a failure.
    pub(crate) fn outcome(&self, result: JobResult) -> JobResult {
        match result {
            Ok(()) if self.failed > 0 && self.succeeded == 0 => Err(JobError::Fatal(format!(
                "{} item{} could not be processed",
                self.failed,
                if self.failed == 1 { "" } else { "s" }
            ))),
            other => other,
        }
    }
}

pub(crate) fn execute(job: &mut Job<'_>, request: &OperationRequest) -> JobResult {
    match request {
        OperationRequest::Copy {
            sources,
            destination,
        } => transfer::run(job, sources, destination, transfer::Mode::Copy),
        OperationRequest::Move {
            sources,
            destination,
        } => transfer::run(job, sources, destination, transfer::Mode::Move),
        OperationRequest::Delete { items } => remove::delete(job, items),
        OperationRequest::Trash { paths } => remove::trash(job, paths),
        OperationRequest::Restore { items } => remove::restore(job, items),
        OperationRequest::Rename { path, new_name } => simple::rename(job, path, new_name),
        OperationRequest::CreateDirectory { parent, name } => {
            simple::create(job, &parent.join(name), true)
        }
        OperationRequest::CreateFile { parent, name } => {
            simple::create(job, &parent.join(name), false)
        }
    }
}

/// `(files, bytes)` below `path` including itself, without following
/// symlinks. Unreadable directories count as one item.
pub(crate) fn scan(job: &Job<'_>, path: &Path) -> Result<(u64, u64), JobError> {
    let mut stack: Vec<PathBuf> = vec![path.to_path_buf()];
    let (mut files, mut bytes) = (0u64, 0u64);
    while let Some(path) = stack.pop() {
        job.check_cancel()?;
        let Ok(meta) = job.fs.metadata(&path) else {
            continue;
        };
        files += 1;
        bytes += meta.size;
        if meta.entry_type == seshat_domain::operation::EntryType::Directory
            && let Ok(children) = job.fs.read_dir(&path)
        {
            stack.extend(children.into_iter().map(|name| path.join(name)));
        }
    }
    Ok((files, bytes))
}
