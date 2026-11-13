//! Fuzzy Find without its UI (Spec 4 Phases 1 and 2): which folders a
//! scope covers ([`roots`]), how candidates are ranked, the engine that
//! walks and ranks them ([`SearchEngine`]), and the session lifecycle that
//! keeps stale results out ([`SearchSessions`]). A query naming a folder
//! is read by [`PathQuery`], and what a query searches is decided by
//! [`SearchTarget`]. [`ResultList`] keeps the results shown and the
//! highlighted one.
//!
//! Filesystem traversal and fuzzy matching come through the
//! [`TreeWalker`](crate::ports::TreeWalker) and
//! [`FuzzyMatcher`](crate::ports::FuzzyMatcher) ports. Search never changes
//! the workspace or the filesystem; opening a result goes through the
//! existing workspace navigation.

mod engine;
mod path_query;
mod rank;
mod results;
mod scope;
mod session;
mod target;
#[cfg(test)]
mod tests;
mod worker;

pub use engine::{DEFAULT_RESULT_LIMIT, SearchEngine, SearchRequest, SearchSummary};
pub use path_query::PathQuery;
pub use results::ResultList;
pub use scope::{ScopeError, SearchRoot, roots};
pub use session::{SearchSessions, SearchTicket};
pub use target::SearchTarget;
pub use worker::{SearchEvent, SearchListener, SearchUpdate, SearchWorker};
