//! Fuzzy Find (Spec 4): the overlay's state and its searches.
//!
//! The controller owns the open session ([`SearchSessions`]), the worker
//! thread running its searches ([`SearchWorker`]), the latest results and
//! which one is highlighted. It never changes the workspace: opening a
//! result is returned as [`FuzzyOutcome::Open`] and the window carries it
//! out through `WorkspaceService` (ADR 0008). Closing the overlay drops
//! the worker, which cancels its search (§14).

use std::sync::Arc;

use seshat_application::ports::{FuzzyMatcher, TreeWalker, WalkOptions};
use seshat_application::search::{
    DEFAULT_RESULT_LIMIT, SearchEngine, SearchEvent, SearchRequest, SearchRoot, SearchSessions,
    SearchSummary, SearchUpdate, SearchWorker, roots,
};
use seshat_application::workspace::WorkspaceService;
use seshat_domain::entities::Location;
use seshat_domain::search::{SearchQuery, SearchResult, SearchScope, SearchStatus};

use super::{Msg, NoticeCx};
use crate::fuzzy_view::{FuzzyView, FuzzyViewEvent};
use crate::notice::{self, Notice};
use crate::pathbar::address_text;

#[derive(Debug)]
pub enum FuzzyMsg {
    /// Something the user did in the overlay.
    View(FuzzyViewEvent),
    /// Results or the end of a search, from the worker thread.
    Update(SearchUpdate),
}

/// What the window must do after a Fuzzy Find message.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum FuzzyOutcome {
    Stay,
    /// Open this result (and the overlay is now closed).
    Open(Box<SearchResult>),
    /// The overlay closed without opening anything.
    Closed,
}

pub(super) struct FuzzyFindController {
    view: FuzzyView,
    walker: Arc<dyn TreeWalker>,
    matcher: Arc<dyn FuzzyMatcher>,
    sessions: SearchSessions,
    /// Present while the overlay is open.
    worker: Option<SearchWorker>,
    /// The folders searched, fixed when the overlay opens: the workspace
    /// can't change while it is open (decision F12).
    roots: Vec<SearchRoot>,
    query: String,
    results: Vec<SearchResult>,
    /// The highlighted result, by identity, so it stays put while results
    /// update (§4.3).
    highlighted: Option<Location>,
    /// How the latest finished search ended.
    summary: Option<SearchSummary>,
}

impl FuzzyFindController {
    pub(super) fn new(
        view: FuzzyView,
        walker: Arc<dyn TreeWalker>,
        matcher: Arc<dyn FuzzyMatcher>,
    ) -> Self {
        Self {
            view,
            walker,
            matcher,
            sessions: SearchSessions::default(),
            worker: None,
            roots: Vec::new(),
            query: String::new(),
            results: Vec::new(),
            highlighted: None,
            summary: None,
        }
    }

    pub(super) fn is_open(&self) -> bool {
        self.worker.is_some()
    }

    /// Whether `widget` belongs to the overlay.
    pub(super) fn contains(&self, widget: &gtk::Widget) -> bool {
        self.view.contains(widget)
    }

    /// Opens the overlay over the workspace, searching the active panel's
    /// folder (§6.1). The workspace, its active panel and branch are left
    /// exactly as they are (§4.1).
    pub(super) fn open(&mut self, service: &WorkspaceService, cx: &mut NoticeCx<'_>) {
        if self.is_open() {
            return;
        }
        let scope = SearchScope::CurrentDirectory;
        self.roots = match roots(&scope, service.workspace()) {
            Ok(roots) => roots,
            Err(err) => {
                *cx.notice = Some(Notice::from_error(&err));
                return;
            }
        };
        let input = cx.sender.input_sender().clone();
        let engine = SearchEngine::new(self.walker.clone(), self.matcher.clone());
        self.worker = Some(SearchWorker::start(
            engine,
            Box::new(move |update| input.emit(Msg::FuzzyFind(FuzzyMsg::Update(update)))),
        ));
        self.sessions.open();
        self.query.clear();
        self.results.clear();
        self.highlighted = None;
        self.summary = None;
        self.view.show(&scope_text(service.active_location()));
        // Walk the folder now, while the user starts typing: the empty
        // query shows nothing but prepares the entries (§4.3).
        self.search();
        tracing::debug!(roots = self.roots.len(), "fuzzy find opened");
    }

    /// Closes the overlay; its running search is cancelled.
    pub(super) fn close(&mut self) {
        if !self.is_open() {
            return;
        }
        self.sessions.close();
        self.worker = None;
        self.results.clear();
        self.highlighted = None;
        self.view.hide();
        tracing::debug!("fuzzy find closed");
    }

    pub(super) fn update(&mut self, msg: FuzzyMsg) -> FuzzyOutcome {
        if !self.is_open() {
            return FuzzyOutcome::Stay; // a late update after closing
        }
        match msg {
            FuzzyMsg::View(FuzzyViewEvent::Changed(text)) => {
                if text != self.query {
                    self.query = text;
                    self.search();
                }
            }
            FuzzyMsg::View(FuzzyViewEvent::Move(step)) => self.move_highlight(step),
            FuzzyMsg::View(FuzzyViewEvent::Highlight(position)) => {
                self.highlighted = self
                    .results
                    .get(position as usize)
                    .map(|r| r.location.clone());
            }
            FuzzyMsg::View(FuzzyViewEvent::Activate) => {
                let Some(result) = self.highlighted_result().cloned() else {
                    return FuzzyOutcome::Stay;
                };
                self.close();
                return FuzzyOutcome::Open(Box::new(result));
            }
            FuzzyMsg::View(FuzzyViewEvent::Close) => {
                self.close();
                return FuzzyOutcome::Closed;
            }
            FuzzyMsg::Update(update) => self.receive(update),
        }
        FuzzyOutcome::Stay
    }

    /// Starts a search for the current query; the previous one is
    /// superseded and can no longer show anything (§14).
    fn search(&mut self) {
        let (Some(worker), Some(ticket)) = (self.worker.as_ref(), self.sessions.search()) else {
            return;
        };
        worker.submit(
            ticket,
            SearchRequest {
                roots: self.roots.clone(),
                query: SearchQuery::new(self.query.clone()),
                options: WalkOptions::default(),
                limit: DEFAULT_RESULT_LIMIT,
            },
        );
        self.render_status();
    }

    fn receive(&mut self, update: SearchUpdate) {
        match update.event {
            SearchEvent::Results(results) => {
                if !self.sessions.accepts(&update.ticket) {
                    return; // superseded: never shown
                }
                self.results = results;
                let index = self.highlight_index();
                self.highlighted = index.map(|i| self.results[i].location.clone());
                self.view.set_results(&self.results, index);
            }
            SearchEvent::Finished(summary) => {
                if !self.sessions.finish(&update.ticket, summary.status.clone()) {
                    return;
                }
                if summary.status == SearchStatus::Completed && !self.query.trim().is_empty() {
                    tracing::debug!(
                        results = self.results.len(),
                        candidates = summary.candidates,
                        "fuzzy find results"
                    );
                }
                self.summary = Some(summary);
            }
        }
        self.render_status();
    }

    fn render_status(&self) {
        self.view.set_status(&status_text(
            &self.query,
            &self.sessions.status(),
            self.results.len(),
            self.summary.as_ref(),
        ));
    }

    /// Where the highlight goes after the results changed: the same
    /// result if it is still there, otherwise the best one.
    fn highlight_index(&self) -> Option<usize> {
        if self.results.is_empty() {
            return None;
        }
        self.highlighted
            .as_ref()
            .and_then(|h| self.results.iter().position(|r| &r.location == h))
            .or(Some(0))
    }

    fn highlighted_result(&self) -> Option<&SearchResult> {
        self.highlight_index().map(|i| &self.results[i])
    }

    fn move_highlight(&mut self, step: isize) {
        let Some(current) = self.highlight_index() else {
            return;
        };
        let last = self.results.len() - 1;
        let next = current.saturating_add_signed(step).min(last);
        self.highlighted = Some(self.results[next].location.clone());
        self.view.set_highlight(Some(next));
    }
}

/// What the overlay says it searches.
fn scope_text(folder: &Location) -> String {
    format!("In {}", address_text(folder))
}

/// The status line: what the search is doing, or what it found.
fn status_text(
    query: &str,
    status: &SearchStatus,
    shown: usize,
    summary: Option<&SearchSummary>,
) -> String {
    if query.trim().is_empty() {
        return match status {
            SearchStatus::Failed(message) => Notice::from_error(message).text().to_owned(),
            _ => "Type to search".to_owned(),
        };
    }
    let mut text = match status {
        SearchStatus::Failed(message) => return Notice::from_error(message).text().to_owned(),
        SearchStatus::Searching if shown == 0 => "Searching…".to_owned(),
        SearchStatus::Searching => format!("{} so far…", notice::count(shown, "result", "results")),
        _ if shown == 0 => "No matches".to_owned(),
        _ if shown >= DEFAULT_RESULT_LIMIT => format!("The best {shown} results"),
        _ => notice::count(shown, "result", "results"),
    };
    if let Some(summary) = summary {
        if summary.unreadable > 0 {
            text.push_str(&format!(
                " · {} couldn't be read",
                notice::count(summary.unreadable, "folder", "folders")
            ));
        }
        if summary.truncated {
            text.push_str(&format!(
                " · only the first {} entries were searched",
                summary.candidates
            ));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(unreadable: usize, truncated: bool) -> SearchSummary {
        SearchSummary {
            status: SearchStatus::Completed,
            candidates: 200_000,
            unreadable,
            truncated,
        }
    }

    #[test]
    fn the_status_line_says_what_the_search_is_doing() {
        let done = SearchStatus::Completed;
        assert_eq!(
            status_text("", &SearchStatus::Searching, 0, None),
            "Type to search"
        );
        assert_eq!(
            status_text("dcm", &SearchStatus::Searching, 0, None),
            "Searching…"
        );
        assert_eq!(
            status_text("dcm", &SearchStatus::Searching, 3, None),
            "3 results so far…"
        );
        assert_eq!(status_text("dcm", &done, 0, None), "No matches");
        assert_eq!(status_text("dcm", &done, 1, None), "1 result");
        assert_eq!(
            status_text("dcm", &done, DEFAULT_RESULT_LIMIT, None),
            "The best 500 results"
        );
        assert_eq!(
            status_text("dcm", &done, 2, Some(&summary(1, true))),
            "2 results · 1 folder couldn't be read · only the first 200000 entries were searched"
        );
        assert_eq!(
            status_text(
                "x",
                &SearchStatus::Failed("“/gone” does not exist".into()),
                0,
                None
            ),
            "“/gone” does not exist."
        );
    }
}
