//! The results a Fuzzy Find session shows and which one is highlighted
//! (§4.3, §8). The highlight follows a result by its identity, so it stays
//! put while newer results replace older ones.

use seshat_domain::entities::Location;
use seshat_domain::search::SearchResult;

/// The results shown, best first, and the highlighted one.
#[derive(Debug, Clone, Default)]
pub struct ResultList {
    results: Vec<SearchResult>,
    /// By identity (the path), never by position.
    highlighted: Option<Location>,
}

impl ResultList {
    pub fn results(&self) -> &[SearchResult] {
        &self.results
    }

    pub fn len(&self) -> usize {
        self.results.len()
    }

    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
    }

    /// Shows `results` instead. The highlight stays on the same result if
    /// it is still there, otherwise it goes to the best one. Returns the
    /// highlighted position.
    pub fn replace(&mut self, results: Vec<SearchResult>) -> Option<usize> {
        self.results = results;
        let index = self.highlight_index();
        self.highlighted = index.map(|i| self.results[i].location.clone());
        index
    }

    /// No results, and nothing highlighted.
    pub fn clear(&mut self) {
        self.results.clear();
        self.highlighted = None;
    }

    /// Highlights the result at `index` (a click); nothing is highlighted
    /// if there is none.
    pub fn select(&mut self, index: usize) {
        self.highlighted = self.results.get(index).map(|r| r.location.clone());
    }

    /// Moves the highlight by `step`, stopping at either end. Returns the
    /// new position, or `None` when there is nothing to highlight.
    pub fn move_by(&mut self, step: isize) -> Option<usize> {
        let current = self.highlight_index()?;
        let next = current
            .saturating_add_signed(step)
            .min(self.results.len() - 1);
        self.highlighted = Some(self.results[next].location.clone());
        Some(next)
    }

    /// The highlighted result's position: the remembered one if it is still
    /// listed, otherwise the best.
    pub fn highlight_index(&self) -> Option<usize> {
        if self.results.is_empty() {
            return None;
        }
        self.highlighted
            .as_ref()
            .and_then(|h| self.results.iter().position(|r| &r.location == h))
            .or(Some(0))
    }

    pub fn highlighted(&self) -> Option<&SearchResult> {
        self.highlight_index().map(|i| &self.results[i])
    }
}

#[cfg(test)]
mod tests {
    use seshat_domain::entities::EntryKind;
    use seshat_domain::search::SearchMatch;

    use super::*;

    fn found(names: &[&str]) -> Vec<SearchResult> {
        names
            .iter()
            .map(|name| SearchResult {
                location: Location::Path(format!("/r/{name}").into()),
                kind: EntryKind::File,
                name: (*name).to_owned(),
                relative_path: (*name).to_owned(),
                origin: None,
                score: 1,
                name_match: SearchMatch::default(),
                path_match: SearchMatch::default(),
            })
            .collect()
    }

    fn highlighted(list: &ResultList) -> Option<&str> {
        list.highlighted().map(|r| r.name.as_str())
    }

    #[test]
    fn the_best_result_is_highlighted_first() {
        let mut list = ResultList::default();
        assert_eq!(list.highlighted(), None);
        assert_eq!(list.replace(found(&["a", "b"])), Some(0));
        assert_eq!(highlighted(&list), Some("a"));
        assert_eq!(list.replace(Vec::new()), None);
        assert_eq!(list.highlighted(), None);
    }

    #[test]
    fn the_highlight_follows_its_result_as_results_update() {
        let mut list = ResultList::default();
        list.replace(found(&["a", "b", "c"]));
        list.move_by(2);
        assert_eq!(highlighted(&list), Some("c"));
        assert_eq!(list.replace(found(&["c", "a"])), Some(0), "c moved up");
        assert_eq!(highlighted(&list), Some("c"));
        assert_eq!(list.replace(found(&["a", "b"])), Some(0), "c went away");
        assert_eq!(highlighted(&list), Some("a"));
    }

    #[test]
    fn moving_stops_at_either_end() {
        let mut list = ResultList::default();
        assert_eq!(list.move_by(1), None, "nothing to move through");
        list.replace(found(&["a", "b", "c"]));
        assert_eq!(list.move_by(10), Some(2));
        assert_eq!(list.move_by(1), Some(2));
        assert_eq!(list.move_by(-10), Some(0));
        assert_eq!(list.move_by(-1), Some(0));
    }

    #[test]
    fn a_click_highlights_that_result() {
        let mut list = ResultList::default();
        list.replace(found(&["a", "b"]));
        list.select(1);
        assert_eq!(highlighted(&list), Some("b"));
        list.select(7);
        assert_eq!(highlighted(&list), Some("a"), "out of range: the best");
        list.clear();
        assert!(list.is_empty());
        assert_eq!(list.highlighted(), None);
    }
}
