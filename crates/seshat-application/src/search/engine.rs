//! Runs a search: walks the scope's folders once, then ranks the cached
//! candidates for every query typed while the session stays open (§11).
//! Synchronous and single-threaded; the caller runs it on a worker thread
//! and stops it with a [`CancelToken`].

use std::sync::Arc;

use seshat_domain::search::{SearchQuery, SearchResult, SearchStatus};

use super::rank::{self, TopRanks};
use super::scope::SearchRoot;
use crate::operations::CancelToken;
use crate::ports::{Candidate, FuzzyMatcher, QueryMatcher, TreeWalker, WalkOptions};

/// Results shown at most by default.
pub const DEFAULT_RESULT_LIMIT: usize = 500;

/// While walking, results are published again after this many new
/// candidates, so they stream in (§11).
const PUBLISH_EVERY: usize = 4096;

/// How often a ranking pass checks for cancellation.
const CANCEL_CHECK_EVERY: usize = 1024;

/// What to search for and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub roots: Vec<SearchRoot>,
    pub query: SearchQuery,
    pub options: WalkOptions,
    /// Results kept at most (the best ones).
    pub limit: usize,
}

/// How a search ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSummary {
    pub status: SearchStatus,
    /// Entries found below the roots (matching or not).
    pub candidates: usize,
    /// Folders skipped because they could not be read.
    pub unreadable: usize,
    /// The walk stopped at `max_entries`.
    pub truncated: bool,
}

impl SearchSummary {
    /// A search that failed before it had any candidates.
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            status: SearchStatus::Failed(message.into()),
            candidates: 0,
            unreadable: 0,
            truncated: false,
        }
    }
}

#[derive(Debug)]
struct Found {
    root: usize,
    candidate: Candidate,
}

/// Candidates of a completed walk, reused while roots and options stay
/// the same. Only for the lifetime of the engine: no persistent index.
#[derive(Debug)]
struct Cache {
    roots: Vec<SearchRoot>,
    options: WalkOptions,
    found: Vec<Found>,
    unreadable: usize,
    truncated: bool,
}

impl Cache {
    /// How a search over these candidates ended.
    fn summary(&self, status: SearchStatus) -> SearchSummary {
        SearchSummary {
            status,
            candidates: self.found.len(),
            unreadable: self.unreadable,
            truncated: self.truncated,
        }
    }
}

/// The search engine of one Fuzzy Find session.
pub struct SearchEngine {
    walker: Arc<dyn TreeWalker>,
    matcher: Arc<dyn FuzzyMatcher>,
    cache: Option<Cache>,
}

impl std::fmt::Debug for SearchEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchEngine")
            .field("cached", &self.cache.as_ref().map(|c| c.found.len()))
            .finish_non_exhaustive()
    }
}

impl SearchEngine {
    pub fn new(walker: Arc<dyn TreeWalker>, matcher: Arc<dyn FuzzyMatcher>) -> Self {
        Self {
            walker,
            matcher,
            cache: None,
        }
    }

    /// Forgets the walked candidates, so the next search walks again. Used
    /// after a search panicked, when the engine is no longer trusted.
    pub fn invalidate(&mut self) {
        self.cache = None;
    }

    /// Runs `request`. `publish` receives the best results so far, best
    /// first; its last call holds the final results of a completed search.
    /// Nothing is published once `cancel` is set, so a superseded search
    /// can never overwrite newer results.
    pub fn run(
        &mut self,
        request: &SearchRequest,
        cancel: &CancelToken,
        publish: &mut dyn FnMut(Vec<SearchResult>),
    ) -> SearchSummary {
        let mut matcher = self.matcher.compile(&request.query);
        let cached = self
            .cache
            .as_ref()
            .is_some_and(|c| c.roots == request.roots && c.options == request.options);
        if !cached && let Err(summary) = self.walk(request, matcher.as_mut(), cancel, publish) {
            return summary;
        }
        let cache = self.cache.as_ref().expect("walked above");
        let summary = |status| cache.summary(status);
        let mut top = TopRanks::new(request.limit);
        if !request.query.is_empty() {
            for (index, found) in cache.found.iter().enumerate() {
                if index % CANCEL_CHECK_EVERY == 0 && cancel.is_cancelled() {
                    return summary(SearchStatus::Cancelled);
                }
                let root = &request.roots[found.root];
                if let Some(r) = rank::rank(
                    matcher.as_mut(),
                    &request.query,
                    &found.candidate,
                    root,
                    index,
                ) {
                    top.push(r);
                }
            }
        }
        if cancel.is_cancelled() {
            return summary(SearchStatus::Cancelled);
        }
        publish(results(
            matcher.as_mut(),
            &top,
            &cache.found,
            &request.roots,
        ));
        summary(SearchStatus::Completed)
    }

    /// Walks the roots, publishing results as they come, and caches the
    /// candidates. Returns the summary to end with if the walk did not
    /// complete.
    fn walk(
        &mut self,
        request: &SearchRequest,
        matcher: &mut dyn QueryMatcher,
        cancel: &CancelToken,
        publish: &mut dyn FnMut(Vec<SearchResult>),
    ) -> Result<(), SearchSummary> {
        self.cache = None;
        let mut found: Vec<Found> = Vec::new();
        let mut top = TopRanks::new(request.limit);
        let mut unreadable = 0;
        let mut truncated = false;
        let mut errors = Vec::new();
        let mut since_publish = 0;
        for (root_index, root) in request.roots.iter().enumerate() {
            let mut options = request.options.clone();
            options.max_entries = request.options.max_entries.saturating_sub(found.len());
            let walked = self
                .walker
                .walk(&root.path, &options, cancel, &mut |candidate| {
                    let index = found.len();
                    if !request.query.is_empty()
                        && let Some(r) =
                            rank::rank(matcher, &request.query, &candidate, root, index)
                    {
                        top.push(r);
                    }
                    found.push(Found {
                        root: root_index,
                        candidate,
                    });
                    since_publish += 1;
                    if since_publish >= PUBLISH_EVERY && !cancel.is_cancelled() {
                        since_publish = 0;
                        publish(results(matcher, &top, &found, &request.roots));
                    }
                });
            match walked {
                Ok(stats) => {
                    unreadable += stats.unreadable;
                    truncated |= stats.truncated;
                }
                Err(err) => {
                    tracing::debug!(error = %err, "search root skipped");
                    errors.push(err);
                }
            }
            if cancel.is_cancelled() || truncated {
                break;
            }
        }
        let walked = Cache {
            roots: request.roots.clone(),
            options: request.options.clone(),
            found,
            unreadable,
            truncated,
        };
        if cancel.is_cancelled() {
            return Err(walked.summary(SearchStatus::Cancelled));
        }
        if errors.len() == request.roots.len()
            && let Some(first) = errors.first()
        {
            return Err(walked.summary(SearchStatus::Failed(first.to_string())));
        }
        self.cache = Some(walked);
        Ok(())
    }
}

fn results(
    matcher: &mut dyn QueryMatcher,
    top: &TopRanks,
    found: &[Found],
    roots: &[SearchRoot],
) -> Vec<SearchResult> {
    top.sorted()
        .iter()
        .map(|r| {
            let f = &found[r.index];
            rank::result(matcher, r, &f.candidate, &roots[f.root])
        })
        .collect()
}
