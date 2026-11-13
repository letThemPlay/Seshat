//! Ranking (Spec 4 §5.1). Documented in `docs/architecture/search.md`.
//!
//! Each candidate is matched against its **name** first and, only if the
//! name doesn't match, against its **path relative to the searched
//! folder**. Results are ordered by, in turn:
//!
//! 1. tier: an exact name match (ignoring case), then any name match, then
//!    a path-only match;
//! 2. the matcher's score;
//! 3. the shorter relative path;
//! 4. the path's bytes, so equal results always come out in the same
//!    order.
//!
//! File metadata (size, dates) never affects the order.

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::path::{Path, PathBuf};

use seshat_domain::entities::Location;
use seshat_domain::search::{SearchMatch, SearchQuery, SearchResult};

use super::scope::SearchRoot;
use crate::ports::{Candidate, QueryMatcher};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Path,
    Name,
    ExactName,
}

/// A candidate's rank, without the match ranges (computed only for the
/// results actually shown). Greater is better.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Rank {
    tier: Tier,
    score: u32,
    path: PathBuf,
    relative_len: usize,
    /// Index into the engine's candidate list.
    pub(super) index: usize,
}

impl Ord for Rank {
    fn cmp(&self, other: &Self) -> Ordering {
        self.tier
            .cmp(&other.tier)
            .then(self.score.cmp(&other.score))
            .then(other.relative_len.cmp(&self.relative_len))
            .then_with(|| other.path.as_os_str().cmp(self.path.as_os_str()))
    }
}

impl PartialOrd for Rank {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn relative_of(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Whether `name` is exactly the query, with case as the query says.
fn is_exact(name: &str, query: &SearchQuery) -> bool {
    let wanted = query.text.trim();
    if query.respects_case() {
        name == wanted
    } else {
        name.to_lowercase() == wanted.to_lowercase()
    }
}

/// Ranks one candidate, or `None` if neither its name nor its path matches.
pub(super) fn rank(
    matcher: &mut dyn QueryMatcher,
    query: &SearchQuery,
    candidate: &Candidate,
    root: &SearchRoot,
    index: usize,
) -> Option<Rank> {
    let name = name_of(&candidate.path);
    let relative = relative_of(&candidate.path, &root.path);
    let (tier, score) = match matcher.score(&name, None) {
        Some(score) if is_exact(&name, query) => (Tier::ExactName, score),
        Some(score) => (Tier::Name, score),
        None => (Tier::Path, matcher.score(&relative, None)?),
    };
    Some(Rank {
        tier,
        score,
        path: candidate.path.clone(),
        relative_len: relative.len(),
        index,
    })
}

/// The full result for a ranked candidate, with its match ranges.
pub(super) fn result(
    matcher: &mut dyn QueryMatcher,
    rank: &Rank,
    candidate: &Candidate,
    root: &SearchRoot,
) -> SearchResult {
    let name = name_of(&candidate.path);
    let relative_path = relative_of(&candidate.path, &root.path);
    let mut name_match = SearchMatch::default();
    let mut path_match = SearchMatch::default();
    if rank.tier == Tier::Path {
        matcher.score(&relative_path, Some(&mut path_match.ranges));
    } else {
        matcher.score(&name, Some(&mut name_match.ranges));
    }
    SearchResult {
        location: Location::Path(candidate.path.clone()),
        kind: candidate.kind,
        name,
        relative_path,
        origin: root.origin,
        score: rank.score,
        name_match,
        path_match,
    }
}

/// The best `limit` ranks seen so far; memory stays bounded however many
/// candidates match (§11).
#[derive(Debug)]
pub(super) struct TopRanks {
    limit: usize,
    heap: BinaryHeap<Reverse<Rank>>,
}

impl TopRanks {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit,
            heap: BinaryHeap::with_capacity(limit.saturating_add(1).min(4096)),
        }
    }

    pub(super) fn push(&mut self, rank: Rank) {
        if self.limit == 0 {
            return;
        }
        if self.heap.len() < self.limit {
            self.heap.push(Reverse(rank));
        } else if self.heap.peek().is_some_and(|Reverse(worst)| rank > *worst) {
            self.heap.pop();
            self.heap.push(Reverse(rank));
        }
    }

    /// Best first.
    pub(super) fn sorted(&self) -> Vec<Rank> {
        let mut ranks: Vec<Rank> = self.heap.iter().map(|Reverse(r)| r.clone()).collect();
        ranks.sort_by(|a, b| b.cmp(a));
        ranks
    }
}
