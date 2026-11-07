//! The File Operation Manager: single source of truth for operation state.
//!
//! Independent of UI and IPC. Both observe it through [`OperationManager::subscribe`]
//! and [`OperationManager::snapshot`].

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant, SystemTime};

use seshat_domain::operation::{
    Conflict, ConflictDecision, ConflictResolution, Operation, OperationId, OperationStatus,
};
use thiserror::Error;

use super::cancel::CancelToken;
use super::events::{
    EventSubscriber, OperationEvent, OperationEventKind, OperationsSnapshot, SubscriptionId,
};
use super::filesystem::FileSystem;
use super::job::{self, Job};
use super::request::{OperationRequest, RequestError};

/// Minimum interval between progress events of one operation. Lifecycle
/// events are never throttled.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Finished operations kept for `get` and late observers.
const FINISHED_HISTORY: usize = 64;

/// Decides conflicts on behalf of the user. Called on a worker thread; may
/// block until the user answers, but must return promptly once `cancel` is
/// set.
pub trait ConflictResolver: Send + Sync {
    fn resolve(
        &self,
        operation: &Operation,
        conflict: &Conflict,
        cancel: &CancelToken,
    ) -> ConflictDecision;
}

/// Used until a UI installs its own resolver: never overwrites anything.
struct SkipConflicts;

impl ConflictResolver for SkipConflicts {
    fn resolve(&self, _: &Operation, conflict: &Conflict, _: &CancelToken) -> ConflictDecision {
        tracing::warn!(target = %conflict.target.display(), "no conflict resolver; skipping");
        ConflictDecision {
            resolution: ConflictResolution::Skip,
            apply_to_all: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CancelError {
    #[error("no such operation")]
    NotFound,
    #[error("the operation has already finished")]
    AlreadyFinished,
}

struct Entry {
    operation: Operation,
    cancel: CancelToken,
    request: Option<OperationRequest>,
    last_progress_event: Option<Instant>,
}

struct State {
    /// Unfinished operations in submission order.
    active: Vec<Entry>,
    finished: VecDeque<Operation>,
    running: usize,
}

struct Events {
    sequence: u64,
    next_subscription: u64,
    subscribers: Vec<(SubscriptionId, EventSubscriber)>,
}

pub(crate) struct Inner {
    pub(crate) fs: Arc<dyn FileSystem>,
    resolver: RwLock<Arc<dyn ConflictResolver>>,
    max_concurrent: usize,
    state: Mutex<State>,
    events: Mutex<Events>,
}

/// Cheap to clone; all clones share the same state.
#[derive(Clone)]
pub struct OperationManager {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for OperationManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OperationManager").finish_non_exhaustive()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panicking worker must not take the whole manager down with it.
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl OperationManager {
    /// `max_concurrent` operations run at once (at least 1); the rest queue.
    pub fn new(fs: Arc<dyn FileSystem>, max_concurrent: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                fs,
                resolver: RwLock::new(Arc::new(SkipConflicts)),
                max_concurrent: max_concurrent.max(1),
                state: Mutex::new(State {
                    active: Vec::new(),
                    finished: VecDeque::new(),
                    running: 0,
                }),
                events: Mutex::new(Events {
                    sequence: 0,
                    next_subscription: 0,
                    subscribers: Vec::new(),
                }),
            }),
        }
    }

    pub fn set_conflict_resolver(&self, resolver: Arc<dyn ConflictResolver>) {
        *self
            .inner
            .resolver
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = resolver;
    }

    pub fn subscribe(&self, subscriber: EventSubscriber) -> SubscriptionId {
        let mut events = lock(&self.inner.events);
        let id = SubscriptionId(events.next_subscription);
        events.next_subscription += 1;
        events.subscribers.push((id, subscriber));
        id
    }

    pub fn unsubscribe(&self, id: SubscriptionId) {
        lock(&self.inner.events)
            .subscribers
            .retain(|(sid, _)| *sid != id);
    }

    /// Validates and queues a request. Invalid requests create nothing.
    pub fn submit(&self, request: OperationRequest) -> Result<OperationId, RequestError> {
        request.validate()?;
        let id = OperationId::new();
        let operation = Operation::new(
            id,
            request.kind(),
            request.sources(),
            request.destination(),
            SystemTime::now(),
        );
        tracing::info!(operation = %id, kind = request.kind().id(), items = operation.sources.len(), "operation queued");

        let mut state = lock(&self.inner.state);
        state.active.push(Entry {
            operation: operation.clone(),
            cancel: CancelToken::new(),
            request: Some(request),
            last_progress_event: None,
        });
        self.inner
            .emit(&state, OperationEventKind::Queued, &operation);
        self.inner.clone().start_queued(&mut state);
        Ok(id)
    }

    /// Requests cancellation. Queued operations end immediately; running
    /// ones stop at the next safe point.
    pub fn cancel(&self, id: OperationId) -> Result<(), CancelError> {
        let mut state = lock(&self.inner.state);
        let Some(index) = state.active.iter().position(|e| e.operation.id() == id) else {
            return if state.finished.iter().any(|op| op.id() == id) {
                Err(CancelError::AlreadyFinished)
            } else {
                Err(CancelError::NotFound)
            };
        };
        let entry = &mut state.active[index];
        if entry.operation.status() == OperationStatus::Cancelling {
            return Ok(());
        }
        entry.cancel.cancel();
        if entry.operation.request_cancel().is_err() {
            return Err(CancelError::AlreadyFinished);
        }
        tracing::info!(operation = %id, "cancellation requested");
        let snapshot = entry.operation.clone();
        let never_started = entry.request.is_some();
        self.inner
            .emit(&state, OperationEventKind::Cancelling, &snapshot);

        if never_started {
            // Still queued: no worker owns it, so finish it here.
            let mut entry = state.active.remove(index);
            let _ = entry.operation.mark_cancelled(SystemTime::now());
            self.inner
                .emit(&state, OperationEventKind::Cancelled, &entry.operation);
            push_finished(&mut state, entry.operation);
        }
        Ok(())
    }

    pub fn get(&self, id: OperationId) -> Option<Operation> {
        let state = lock(&self.inner.state);
        state
            .active
            .iter()
            .map(|e| &e.operation)
            .chain(state.finished.iter())
            .find(|op| op.id() == id)
            .cloned()
    }

    /// Unfinished operations, consistent with the event sequence.
    pub fn snapshot(&self) -> OperationsSnapshot {
        let state = lock(&self.inner.state);
        let events = lock(&self.inner.events);
        OperationsSnapshot {
            sequence: events.sequence,
            operations: state.active.iter().map(|e| e.operation.clone()).collect(),
        }
    }

    pub fn active_count(&self) -> usize {
        lock(&self.inner.state).active.len()
    }
}

fn push_finished(state: &mut State, operation: Operation) {
    state.finished.push_back(operation);
    while state.finished.len() > FINISHED_HISTORY {
        state.finished.pop_front();
    }
}

impl Inner {
    /// Delivers an event. Callers hold the state lock, so the event order
    /// matches the order of state changes, and snapshots (which take both
    /// locks) see a sequence consistent with the operations they return.
    fn emit(
        &self,
        _state: &MutexGuard<'_, State>,
        kind: OperationEventKind,
        operation: &Operation,
    ) {
        let mut events = lock(&self.events);
        events.sequence += 1;
        let event = OperationEvent {
            sequence: events.sequence,
            kind,
            operation: operation.clone(),
        };
        for (_, subscriber) in &events.subscribers {
            subscriber(&event);
        }
    }

    /// Starts queued operations while capacity allows.
    fn start_queued(self: Arc<Self>, state: &mut MutexGuard<'_, State>) {
        while state.running < self.max_concurrent {
            let Some(entry) = state.active.iter_mut().find(|e| e.request.is_some()) else {
                return;
            };
            let request = entry.request.take().expect("queued entry has a request");
            let id = entry.operation.id();
            let cancel = entry.cancel.clone();
            if entry.operation.start(SystemTime::now()).is_err() {
                continue;
            }
            let snapshot = entry.operation.clone();
            state.running += 1;
            self.emit(state, OperationEventKind::Started, &snapshot);

            let inner = self.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("seshat-op-{id}"))
                .spawn(move || inner.run(id, request, cancel));
            if let Err(err) = spawned {
                tracing::error!(operation = %id, error = %err, "cannot start worker thread");
                state.running -= 1;
                self.finish_locked(state, id, Err(job::JobError::Fatal(err.to_string())));
            }
        }
    }

    fn run(self: Arc<Self>, id: OperationId, request: OperationRequest, cancel: CancelToken) {
        let mut job = Job::new(&self, id, cancel);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            job::execute(&mut job, &request)
        }))
        .unwrap_or_else(|_| Err(job::JobError::Fatal("internal error".to_owned())));
        let outcome = job.outcome(result);

        let mut state = lock(&self.state);
        state.running -= 1;
        self.finish_locked(&mut state, id, outcome);
        self.clone().start_queued(&mut state);
    }

    fn finish_locked(
        &self,
        state: &mut MutexGuard<'_, State>,
        id: OperationId,
        outcome: Result<(), job::JobError>,
    ) {
        let Some(index) = state.active.iter().position(|e| e.operation.id() == id) else {
            return;
        };
        let mut entry = state.active.remove(index);
        let op = &mut entry.operation;
        let now = SystemTime::now();
        let kind = match outcome {
            Ok(()) => {
                // Completed is valid from both InProgress and Cancelling.
                if op.status() == OperationStatus::Started {
                    let _ = op.begin_work();
                }
                let _ = op.complete(now);
                OperationEventKind::Completed
            }
            Err(job::JobError::Cancelled) => {
                // Cancelled via a conflict dialog: report Cancelling first so
                // observers always see the full lifecycle.
                if op.status() != OperationStatus::Cancelling && op.request_cancel().is_ok() {
                    let cancelling = op.clone();
                    self.emit(state, OperationEventKind::Cancelling, &cancelling);
                }
                let _ = op.mark_cancelled(now);
                OperationEventKind::Cancelled
            }
            Err(job::JobError::Fatal(message)) => {
                let _ = op.fail(message, now);
                OperationEventKind::Failed
            }
        };
        tracing::info!(
            operation = %id,
            status = ?op.status(),
            failures = op.failures.len(),
            error = op.error.as_deref().unwrap_or(""),
            "operation finished"
        );
        let snapshot = op.clone();
        self.emit(state, kind, &snapshot);
        push_finished(state, entry.operation);
    }

    /// Applies `change` to an operation's state and emits an event: always
    /// when `important`, otherwise at most every [`PROGRESS_INTERVAL`].
    pub(crate) fn update(
        &self,
        id: OperationId,
        important: bool,
        change: impl FnOnce(&mut Operation),
    ) {
        let mut state = lock(&self.state);
        let Some(entry) = state.active.iter_mut().find(|e| e.operation.id() == id) else {
            return;
        };
        change(&mut entry.operation);
        let now = Instant::now();
        let due = entry
            .last_progress_event
            .is_none_or(|last| now.duration_since(last) >= PROGRESS_INTERVAL);
        if important || due {
            entry.last_progress_event = Some(now);
            let snapshot = entry.operation.clone();
            self.emit(&state, OperationEventKind::Progress, &snapshot);
        }
    }

    pub(crate) fn snapshot_of(&self, id: OperationId) -> Option<Operation> {
        lock(&self.state)
            .active
            .iter()
            .find(|e| e.operation.id() == id)
            .map(|e| e.operation.clone())
    }

    pub(crate) fn resolver(&self) -> Arc<dyn ConflictResolver> {
        self.resolver
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}
