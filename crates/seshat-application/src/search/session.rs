//! Search sessions and their lifecycle (§14): each session has a stable
//! id, every new query starts a new generation, and only the latest
//! generation of the open session may publish results.

use seshat_domain::search::{SearchSessionId, SearchStatus};

use crate::operations::CancelToken;

/// Permission to run one search: the session and generation it belongs
/// to, and the token that stops it when superseded.
#[derive(Debug, Clone)]
pub struct SearchTicket {
    pub session: SearchSessionId,
    pub generation: u64,
    pub cancel: CancelToken,
}

#[derive(Debug)]
struct Current {
    id: SearchSessionId,
    generation: u64,
    cancel: Option<CancelToken>,
    status: SearchStatus,
}

/// The open Fuzzy Find session, if any. Pure state; owns no threads.
#[derive(Debug, Default)]
pub struct SearchSessions {
    next_id: u64,
    current: Option<Current>,
}

impl SearchSessions {
    /// Opens a new session, closing (and cancelling) any previous one.
    pub fn open(&mut self) -> SearchSessionId {
        self.close();
        self.next_id += 1;
        let id = SearchSessionId::from_raw(self.next_id);
        self.current = Some(Current {
            id,
            generation: 0,
            cancel: None,
            status: SearchStatus::Idle,
        });
        id
    }

    /// Starts a search for a new query in the open session. The previous
    /// search of the session is cancelled: it is superseded. `None` when
    /// no session is open.
    pub fn search(&mut self) -> Option<SearchTicket> {
        let current = self.current.as_mut()?;
        if let Some(previous) = current.cancel.take() {
            previous.cancel();
        }
        current.generation += 1;
        let cancel = CancelToken::new();
        current.cancel = Some(cancel.clone());
        current.status = SearchStatus::Searching;
        Some(SearchTicket {
            session: current.id,
            generation: current.generation,
            cancel,
        })
    }

    /// Whether results of `ticket` may be shown: it is the latest search of
    /// the open session and has not been cancelled.
    pub fn accepts(&self, ticket: &SearchTicket) -> bool {
        self.current.as_ref().is_some_and(|c| {
            c.id == ticket.session
                && c.generation == ticket.generation
                && !ticket.cancel.is_cancelled()
        })
    }

    /// Records how the search of `ticket` ended, if it is still the
    /// latest of the open session. Returns whether it was. A cancelled
    /// ticket always ends as cancelled, even if its work got to finish.
    pub fn finish(&mut self, ticket: &SearchTicket, status: SearchStatus) -> bool {
        match self.current.as_mut() {
            Some(c) if c.id == ticket.session && c.generation == ticket.generation => {
                c.status = if ticket.cancel.is_cancelled() {
                    SearchStatus::Cancelled
                } else {
                    status
                };
                c.cancel = None;
                true
            }
            _ => false,
        }
    }

    /// Stops the running search, keeping the session open.
    pub fn cancel(&mut self) {
        if let Some(current) = self.current.as_mut()
            && let Some(cancel) = current.cancel.take()
        {
            cancel.cancel();
            current.status = SearchStatus::Cancelled;
        }
    }

    /// Closes the session; its running search is cancelled, so it cannot
    /// outlive its owner (§14).
    pub fn close(&mut self) {
        self.cancel();
        self.current = None;
    }

    pub fn current(&self) -> Option<SearchSessionId> {
        self.current.as_ref().map(|c| c.id)
    }

    /// The open session's status; `Idle` when none is open.
    pub fn status(&self) -> SearchStatus {
        self.current
            .as_ref()
            .map(|c| c.status.clone())
            .unwrap_or_default()
    }
}
