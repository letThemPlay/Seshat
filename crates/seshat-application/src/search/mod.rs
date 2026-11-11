//! Fuzzy Find without its UI (Spec 4 Phases 1 and 2): which folders a
//! scope covers ([`roots`]), how candidates are ranked, the engine that
//! walks and ranks them ([`SearchEngine`]), and the session lifecycle that
//! keeps stale results out ([`SearchSessions`]).
//!
//! Filesystem traversal and fuzzy matching come through the
//! [`TreeWalker`](crate::ports::TreeWalker) and
//! [`FuzzyMatcher`](crate::ports::FuzzyMatcher) ports. Search never changes
//! the workspace or the filesystem; opening a result goes through the
//! existing workspace navigation.

mod engine;
mod rank;
mod scope;
mod session;
#[cfg(test)]
mod tests;

pub use engine::{DEFAULT_RESULT_LIMIT, SearchEngine, SearchRequest, SearchSummary};
pub use scope::{ScopeError, SearchRoot, roots};
pub use session::{SearchSessions, SearchTicket};
