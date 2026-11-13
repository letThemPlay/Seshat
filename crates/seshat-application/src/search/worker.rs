//! Runs one Fuzzy Find session's searches on a background thread (§11), so
//! walking and ranking never block the UI.
//!
//! The worker owns the session's [`SearchEngine`] (and so its cache of
//! walked entries). It keeps at most one search waiting: submitting a new
//! one replaces a search that hasn't started, and the caller cancels the
//! running one through its ticket ([`SearchSessions::search`]). Updates go
//! to a listener; the caller drops those whose ticket
//! [`SearchSessions::accepts`] no longer accepts, so stale results never
//! show.
//!
//! Dropping the worker stops it without waiting for it: a walk stuck on a
//! slow mount can never hold up closing Fuzzy Find or quitting.
//!
//! [`SearchSessions::search`]: super::SearchSessions::search
//! [`SearchSessions::accepts`]: super::SearchSessions::accepts

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;

use seshat_domain::search::SearchResult;

use super::engine::{SearchEngine, SearchRequest, SearchSummary};
use super::session::SearchTicket;
use crate::operations::CancelToken;

/// What a search reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchEvent {
    /// The best results so far, best first. Later ones replace earlier.
    Results(Vec<SearchResult>),
    /// The search ended; nothing more comes for its ticket.
    Finished(SearchSummary),
}

/// One report from the worker, for the search `ticket` started.
#[derive(Debug, Clone)]
pub struct SearchUpdate {
    pub ticket: SearchTicket,
    pub event: SearchEvent,
}

/// Receives the worker's updates, on the worker thread.
pub type SearchListener = Box<dyn Fn(SearchUpdate) + Send>;

#[derive(Default)]
struct Slot {
    /// The search to run next; a newer submission replaces it.
    waiting: Option<(SearchTicket, SearchRequest)>,
    /// Stops the search being run, if any.
    running: Option<CancelToken>,
    /// The worker has been dropped: finish and stop.
    closed: bool,
}

struct Shared {
    slot: Mutex<Slot>,
    wake: Condvar,
}

fn lock(mutex: &Mutex<Slot>) -> MutexGuard<'_, Slot> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The background thread of one Fuzzy Find session.
pub struct SearchWorker {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for SearchWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchWorker").finish_non_exhaustive()
    }
}

impl SearchWorker {
    /// Starts the thread. `engine` serves every search of the session.
    pub fn start(engine: SearchEngine, listener: SearchListener) -> Self {
        let shared = Arc::new(Shared {
            slot: Mutex::new(Slot::default()),
            wake: Condvar::new(),
        });
        let worker = shared.clone();
        thread::Builder::new()
            .name("seshat-search".into())
            .spawn(move || run(&worker, engine, &listener))
            .expect("the search thread starts");
        Self { shared }
    }

    /// Queues a search, replacing one that is still waiting (its ticket
    /// has been superseded, so it would show nothing anyway).
    pub fn submit(&self, ticket: SearchTicket, request: SearchRequest) {
        lock(&self.shared.slot).waiting = Some((ticket, request));
        self.shared.wake.notify_one();
    }
}

impl Drop for SearchWorker {
    fn drop(&mut self) {
        let mut slot = lock(&self.shared.slot);
        slot.closed = true;
        if let Some((ticket, _)) = slot.waiting.take() {
            ticket.cancel.cancel();
        }
        if let Some(running) = slot.running.take() {
            running.cancel();
        }
        drop(slot);
        self.shared.wake.notify_one();
    }
}

fn run(shared: &Shared, mut engine: SearchEngine, listener: &SearchListener) {
    loop {
        let (ticket, request) = {
            let mut slot = lock(&shared.slot);
            loop {
                if slot.closed {
                    return;
                }
                if let Some(next) = slot.waiting.take() {
                    slot.running = Some(next.0.cancel.clone());
                    break next;
                }
                slot = shared
                    .wake
                    .wait(slot)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        };
        let summary = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            engine.run(&request, &ticket.cancel, &mut |results| {
                listener(SearchUpdate {
                    ticket: ticket.clone(),
                    event: SearchEvent::Results(results),
                });
            })
        }))
        .unwrap_or_else(|_| {
            // A bug must not take the app down. The engine is not trusted
            // after a panic, so the next search walks again.
            engine.invalidate();
            tracing::error!("the search failed unexpectedly");
            SearchSummary::failed("the search failed unexpectedly")
        });
        lock(&shared.slot).running = None;
        listener(SearchUpdate {
            ticket,
            event: SearchEvent::Finished(summary),
        });
    }
}
