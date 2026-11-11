use std::ops::Range;

use seshat_domain::search::SearchQuery;

/// Fuzzy matching, isolated so the library behind it can be replaced
/// without touching the domain, the search engine or the UI (Spec 4 §13).
pub trait FuzzyMatcher: Send + Sync {
    /// Prepares `query` for matching many strings. An empty query matches
    /// nothing.
    fn compile(&self, query: &SearchQuery) -> Box<dyn QueryMatcher>;
}

/// A query prepared for matching. Used by one search on one thread.
pub trait QueryMatcher: Send {
    /// How well `haystack` matches, or `None` if it doesn't. Higher is
    /// better. With `ranges`, it is cleared and filled with the matched
    /// parts of `haystack` as byte ranges on grapheme boundaries, in order
    /// and merged where adjacent.
    fn score(&mut self, haystack: &str, ranges: Option<&mut Vec<Range<usize>>>) -> Option<u32>;
}
