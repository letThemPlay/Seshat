//! Fuzzy Find value objects: what is searched for, where, and what was
//! found. Independent of any fuzzy-matching library and of the UI.

use std::fmt;
use std::ops::Range;
use std::path::PathBuf;

use crate::entities::{EntryKind, Location};
use crate::workspace::{BranchId, PanelId};

/// How letter case is treated when matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CaseMode {
    /// Case never matters (the default, Spec 4 §4.3).
    #[default]
    Insensitive,
    /// Case matters only if the query contains a capital letter.
    Smart,
    /// Case always matters.
    Sensitive,
}

/// What the user typed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SearchQuery {
    pub text: String,
    pub case: CaseMode,
}

impl SearchQuery {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            case: CaseMode::default(),
        }
    }

    /// An empty query shows nothing rather than every file (§4.3).
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}

/// Where a search looks (§6).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SearchScope {
    /// The active panel's folder, recursively.
    CurrentDirectory,
    /// The active branch's root folder, recursively (decision F2).
    CurrentBranch,
    /// The root folders of every branch, each searched once.
    EntireWorkspace,
    /// A folder given by its absolute path.
    AbsolutePath(PathBuf),
}

/// Identifies one Fuzzy Find session (from opening to closing it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SearchSessionId(u64);

impl SearchSessionId {
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Display for SearchSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "s{}", self.0)
    }
}

/// Where a search stands (§14).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SearchStatus {
    #[default]
    Idle,
    Searching,
    Completed,
    Cancelled,
    Failed(String),
}

/// The panel and branch a result was found under (§6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SearchOrigin {
    pub branch: BranchId,
    pub panel: PanelId,
}

/// The matched parts of a displayed string: byte ranges, always on
/// grapheme boundaries, in order and never adjacent or overlapping. The
/// string itself is never changed (§5.2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchMatch {
    pub ranges: Vec<Range<usize>>,
}

impl SearchMatch {
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }
}

/// One found file or folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    /// Its stable identity: the full path, never the display name (§7).
    pub location: Location,
    pub kind: EntryKind,
    /// The file name, for display (lossy for non-UTF-8 names).
    pub name: String,
    /// The path relative to the searched folder, for display.
    pub relative_path: String,
    pub origin: Option<SearchOrigin>,
    /// Higher is better; only comparable within one query.
    pub score: u32,
    /// Matched parts of `name`, when the name matched.
    pub name_match: SearchMatch,
    /// Matched parts of `relative_path`, when only the path matched.
    pub path_match: SearchMatch,
}
