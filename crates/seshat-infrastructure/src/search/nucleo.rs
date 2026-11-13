//! The fuzzy matcher, backed by `nucleo-matcher` (the matching core of the
//! Helix editor's picker). Swappable: only [`FuzzyMatcher`] is visible to
//! the rest of the application.

use std::ops::Range;

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use seshat_application::ports::{FuzzyMatcher, QueryMatcher};
use seshat_domain::search::{CaseMode, SearchQuery};
use unicode_segmentation::UnicodeSegmentation;

/// Fuzzy matching with path-aware bonuses: consecutive characters, word
/// and `/` boundaries, prefixes. Words of the query (split on spaces) must
/// all match. Accented letters match their plain form (`e` finds `é`).
/// fzf's operators (`^`, `$`, `'`, `!`) are not interpreted: a query is
/// always plain text.
#[derive(Debug, Default, Clone, Copy)]
pub struct NucleoMatcher;

impl NucleoMatcher {
    pub fn new() -> Self {
        Self
    }
}

impl FuzzyMatcher for NucleoMatcher {
    fn compile(&self, query: &SearchQuery) -> Box<dyn QueryMatcher> {
        let case = match query.case {
            CaseMode::Insensitive => CaseMatching::Ignore,
            CaseMode::Smart => CaseMatching::Smart,
            CaseMode::Sensitive => CaseMatching::Respect,
        };
        let pattern = Pattern::new(
            query.text.trim(),
            case,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        Box::new(Compiled {
            empty: query.is_empty(),
            pattern,
            matcher: Matcher::new(Config::DEFAULT.match_paths()),
            chars: Vec::new(),
            indices: Vec::new(),
        })
    }
}

struct Compiled {
    empty: bool,
    pattern: Pattern,
    matcher: Matcher,
    chars: Vec<char>,
    indices: Vec<u32>,
}

impl QueryMatcher for Compiled {
    fn score(&mut self, haystack: &str, ranges: Option<&mut Vec<Range<usize>>>) -> Option<u32> {
        if self.empty {
            return None;
        }
        let text = Utf32Str::new(haystack, &mut self.chars);
        let Some(ranges) = ranges else {
            return self.pattern.score(text, &mut self.matcher);
        };
        self.indices.clear();
        let score = self
            .pattern
            .indices(text, &mut self.matcher, &mut self.indices)?;
        self.indices.sort_unstable();
        self.indices.dedup();
        *ranges = byte_ranges(haystack, &self.indices);
        Some(score)
    }
}

/// Turns nucleo's match indices into byte ranges of whole graphemes.
///
/// `Utf32Str::new` indexes bytes when the text is ASCII, or when the first
/// character of every grapheme is ASCII (it then matches the raw bytes);
/// otherwise it indexes graphemes. Either way, a match is widened to the
/// whole grapheme it lies in, so a highlight never splits a character or
/// separates a letter from its accent (Spec 4 §5.2).
fn byte_ranges(haystack: &str, indices: &[u32]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut push = |range: Range<usize>| match ranges.last_mut() {
        Some(last) if last.end >= range.start => last.end = last.end.max(range.end),
        _ => ranges.push(range),
    };
    if haystack.is_ascii() {
        for &i in indices {
            let i = i as usize;
            push(i..i + 1);
        }
        return ranges;
    }
    let graphemes: Vec<(usize, &str)> = haystack.grapheme_indices(true).collect();
    let by_bytes = graphemes
        .iter()
        .all(|(_, g)| g.chars().next().is_some_and(|c| c.is_ascii()));
    for &i in indices {
        let i = i as usize;
        let grapheme = if by_bytes {
            // The grapheme containing byte `i`.
            graphemes
                .partition_point(|(start, _)| *start <= i)
                .checked_sub(1)
        } else {
            (i < graphemes.len()).then_some(i)
        };
        if let Some(&(start, g)) = grapheme.and_then(|g| graphemes.get(g)) {
            push(start..start + g.len());
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matched(query: &str, haystack: &str) -> Option<(u32, Vec<String>)> {
        let mut compiled = NucleoMatcher.compile(&SearchQuery::new(query));
        let mut ranges = Vec::new();
        let score = compiled.score(haystack, Some(&mut ranges))?;
        assert_eq!(compiled.score(haystack, None), Some(score));
        for r in &ranges {
            assert!(haystack.is_char_boundary(r.start) && haystack.is_char_boundary(r.end));
        }
        Some((
            score,
            ranges
                .iter()
                .map(|r| haystack[r.clone()].to_owned())
                .collect(),
        ))
    }

    #[test]
    fn matches_subsequences_case_insensitively() {
        let (_, parts) = matched("dcm", "Development/Code/Modules").unwrap();
        assert_eq!(parts.concat().to_lowercase(), "dcm");
        assert!(matched("DOC", "documents").is_some());
        assert!(matched("xyz", "documents").is_none());
        assert!(matched("", "documents").is_none(), "empty matches nothing");
        assert!(matched("   ", "documents").is_none());
    }

    #[test]
    fn rewards_consecutive_and_boundary_matches() {
        let (tight, _) = matched("code", "Code").unwrap();
        let (loose, _) = matched("code", "cxoxdxe").unwrap();
        assert!(tight > loose);
        let (boundary, _) = matched("m", "Code/Modules").unwrap();
        let (inner, _) = matched("m", "Codemodules").unwrap();
        assert!(boundary > inner, "a `/` boundary scores higher");
    }

    #[test]
    fn every_word_of_the_query_must_match() {
        assert!(matched("dev mod", "Development/Code/Modules").is_some());
        assert!(matched("dev zzz", "Development/Code/Modules").is_none());
    }

    #[test]
    fn case_modes() {
        let sensitive = |q: &str, h: &str| {
            let query = SearchQuery {
                text: q.into(),
                case: CaseMode::Sensitive,
            };
            NucleoMatcher.compile(&query).score(h, None).is_some()
        };
        assert!(!sensitive("doc", "Doc"));
        assert!(sensitive("Doc", "Doc"));
        let smart = |q: &str, h: &str| {
            let query = SearchQuery {
                text: q.into(),
                case: CaseMode::Smart,
            };
            NucleoMatcher.compile(&query).score(h, None).is_some()
        };
        assert!(smart("doc", "Doc"));
        assert!(!smart("Doc", "doc"));
    }

    #[test]
    fn highlights_whole_characters_in_unicode_names() {
        // Multi-byte letters, an emoji, and accents.
        let (_, parts) = matched("ünï", "Ünïcødé ✓.txt").unwrap();
        assert_eq!(parts, ["Ünï"]);
        let (_, parts) = matched("cd", "Ünïcødé").unwrap();
        assert_eq!(parts, ["c", "d"]);
        // Accent folding: plain letters find accented ones.
        let (_, parts) = matched("cafe", "Café.txt").unwrap();
        assert_eq!(parts, ["Café"]);
        // A combining accent stays with its letter.
        let combining = "Cafe\u{301}.txt";
        let (_, parts) = matched("cafe", combining).unwrap();
        assert_eq!(parts, ["Cafe\u{301}"]);
    }
}
