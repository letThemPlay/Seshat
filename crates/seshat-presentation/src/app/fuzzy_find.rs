//! Fuzzy Find (Spec 4): the overlay's state and its searches.
//!
//! The controller owns the open session ([`SearchSessions`]), the worker
//! thread running its searches ([`SearchWorker`]), the latest results and
//! which one is highlighted. It never changes the workspace: opening or
//! revealing a result is returned as a [`FuzzyOutcome`] and the window
//! carries it out through `WorkspaceService` (ADR 0008). Closing the
//! overlay drops the worker, which cancels its search (§14).
//!
//! What is searched is the scope picked in the overlay (§6.1–6.3), or,
//! when the query starts with `/` or `~`, the folder it names (§6.4,
//! decision F10). The workspace can't change while the overlay is open
//! (F12), so the folders a scope covers and the branch numbers stay valid.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use relm4::ComponentSender;
use seshat_application::ports::{FuzzyMatcher, TreeWalker, WalkOptions};
use seshat_application::search::{
    DEFAULT_RESULT_LIMIT, PathQuery, SearchEngine, SearchEvent, SearchRequest, SearchRoot,
    SearchSessions, SearchSummary, SearchUpdate, SearchWorker, roots,
};
use seshat_application::workspace::WorkspaceService;
use seshat_domain::entities::Location;
use seshat_domain::search::{SearchQuery, SearchResult, SearchScope, SearchStatus};
use seshat_domain::workspace::BranchId;

use super::{App, Msg};
use crate::fuzzy_view::{FuzzyView, FuzzyViewEvent, ResultRow};
use crate::notice::{self, Notice};

#[derive(Debug)]
pub enum FuzzyMsg {
    /// Something the user did in the overlay.
    View(FuzzyViewEvent),
    /// Results or the end of a search, from the worker thread.
    Update(SearchUpdate),
}

/// What the window must do after a Fuzzy Find message. Every outcome
/// but `Stay` means the overlay is now closed.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum FuzzyOutcome {
    Stay,
    /// Open this result (§9.1, §9.2).
    Open(Box<SearchResult>),
    /// Show this result in its folder (§9.4).
    Reveal(Box<SearchResult>),
    /// Open the folder a path query names (F10).
    OpenFolder(Location),
    /// The overlay closed without opening anything.
    Closed,
}

/// What the current text searches.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Target {
    /// `None` while the text names a folder (F10).
    scope: Option<SearchScope>,
    roots: Vec<SearchRoot>,
    /// The text to match: the query, or what follows a named folder.
    pattern: String,
}

pub(super) struct FuzzyFindController {
    view: FuzzyView,
    walker: Arc<dyn TreeWalker>,
    matcher: Arc<dyn FuzzyMatcher>,
    home: PathBuf,
    sessions: SearchSessions,
    /// Present while the overlay is open.
    worker: Option<SearchWorker>,
    /// The scope picked in the overlay. It lasts until Seshat closes and
    /// isn't saved (F12).
    scope: SearchScope,
    /// What is being searched, or why nothing can be (e.g. the Trash).
    target: Result<Target, String>,
    /// Branch numbers, as the branch bar shows them, for the badges.
    branch_numbers: HashMap<BranchId, usize>,
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
        home: PathBuf,
    ) -> Self {
        Self {
            view,
            walker,
            matcher,
            home,
            sessions: SearchSessions::default(),
            worker: None,
            scope: SearchScope::CurrentDirectory,
            target: Err(String::new()),
            branch_numbers: HashMap::new(),
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

    /// Opens the overlay over the workspace, searching the scope picked
    /// last (§6). The workspace, its active panel and branch are left
    /// exactly as they are (§4.1).
    pub(super) fn open(&mut self, service: &WorkspaceService, sender: &ComponentSender<App>) {
        if self.is_open() {
            return;
        }
        let input = sender.input_sender().clone();
        let engine = SearchEngine::new(self.walker.clone(), self.matcher.clone());
        self.worker = Some(SearchWorker::start(
            engine,
            Box::new(move |update| input.emit(Msg::FuzzyFind(FuzzyMsg::Update(update)))),
        ));
        self.sessions.open();
        self.branch_numbers = service
            .workspace()
            .branches()
            .iter()
            .enumerate()
            .map(|(i, branch)| (branch.id(), i + 1))
            .collect();
        self.query.clear();
        self.summary = None;
        self.view.show();
        // Walk the folders now, while the user starts typing: the empty
        // query shows nothing but prepares the entries (§4.3).
        self.search(service);
        tracing::debug!(scope = ?self.scope, "fuzzy find opened");
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

    pub(super) fn update(&mut self, msg: FuzzyMsg, service: &WorkspaceService) -> FuzzyOutcome {
        if !self.is_open() {
            return FuzzyOutcome::Stay; // a late update after closing
        }
        let outcome = match msg {
            FuzzyMsg::View(FuzzyViewEvent::Changed(text)) => {
                if text != self.query {
                    self.query = text;
                    self.search(service);
                }
                FuzzyOutcome::Stay
            }
            FuzzyMsg::View(FuzzyViewEvent::Scope(scope)) => {
                if scope != self.scope {
                    self.scope = scope;
                    tracing::debug!(scope = ?self.scope, "fuzzy find scope");
                    self.search(service);
                }
                FuzzyOutcome::Stay
            }
            FuzzyMsg::View(FuzzyViewEvent::Move(step)) => {
                self.move_highlight(step);
                FuzzyOutcome::Stay
            }
            FuzzyMsg::View(FuzzyViewEvent::Highlight(position)) => {
                self.highlighted = self
                    .results
                    .get(position as usize)
                    .map(|r| r.location.clone());
                FuzzyOutcome::Stay
            }
            FuzzyMsg::View(FuzzyViewEvent::Activate) => self.activate(),
            FuzzyMsg::View(FuzzyViewEvent::Reveal) => match self.highlighted_result() {
                Some(result) => FuzzyOutcome::Reveal(Box::new(result.clone())),
                None => FuzzyOutcome::Stay,
            },
            FuzzyMsg::View(FuzzyViewEvent::Close) => FuzzyOutcome::Closed,
            FuzzyMsg::Update(update) => {
                self.receive(update);
                FuzzyOutcome::Stay
            }
        };
        if outcome != FuzzyOutcome::Stay {
            self.close();
        }
        outcome
    }

    /// Enter: the highlighted result, or with a path query and nothing to
    /// match yet, the folder it names, unless that folder couldn't be read.
    fn activate(&self) -> FuzzyOutcome {
        if let Ok(target) = &self.target
            && target.scope.is_none()
            && target.pattern.trim().is_empty()
        {
            if matches!(self.sessions.status(), SearchStatus::Failed(_)) {
                return FuzzyOutcome::Stay; // the reason is on the status line
            }
            return match target.roots.first() {
                Some(root) => FuzzyOutcome::OpenFolder(Location::Path(root.path.clone())),
                None => FuzzyOutcome::Stay,
            };
        }
        match self.highlighted_result() {
            Some(result) => FuzzyOutcome::Open(Box::new(result.clone())),
            None => FuzzyOutcome::Stay,
        }
    }

    /// What the current text searches: the folder it names (F10), or the
    /// chosen scope's folders.
    fn target(&self, service: &WorkspaceService) -> Result<Target, String> {
        let (scope, chosen, pattern) = match PathQuery::parse(&self.query, &self.home) {
            Some(path) => (SearchScope::AbsolutePath(path.folder), None, path.pattern),
            None => (
                self.scope.clone(),
                Some(self.scope.clone()),
                self.query.clone(),
            ),
        };
        let roots = roots(&scope, service.workspace())
            .map_err(|err| Notice::from_error(&err).text().to_owned())?;
        Ok(Target {
            scope: chosen,
            roots,
            pattern,
        })
    }

    /// Starts a search for the current text; the previous one is
    /// superseded and can no longer show anything (§14).
    fn search(&mut self, service: &WorkspaceService) {
        self.target = self.target(service);
        // The drop-down is greyed out while the text names a folder.
        let (chosen, folders) = match &self.target {
            Ok(target) => (target.scope.as_ref(), folders_text(&target.roots)),
            Err(_) => (Some(&self.scope), String::new()),
        };
        self.view.set_scope(chosen, &folders);
        let (Some(worker), Some(ticket)) = (self.worker.as_ref(), self.sessions.search()) else {
            return;
        };
        match &self.target {
            Ok(target) => worker.submit(
                ticket,
                SearchRequest {
                    roots: target.roots.clone(),
                    query: SearchQuery::new(target.pattern.clone()),
                    options: WalkOptions::default(),
                    limit: DEFAULT_RESULT_LIMIT,
                },
            ),
            Err(_) => {
                // Nothing to search: no results, and the reason is shown.
                self.results.clear();
                self.highlighted = None;
                self.summary = None;
                self.view.set_results(&[], None);
            }
        }
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
                self.view.set_results(&self.rows(), index);
            }
            SearchEvent::Finished(summary) => {
                if !self.sessions.finish(&update.ticket, summary.status.clone()) {
                    return;
                }
                if summary.status == SearchStatus::Completed && !self.pattern().trim().is_empty() {
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

    /// The results as listed, with branch badges when every branch is
    /// searched (§6.3).
    fn rows(&self) -> Vec<ResultRow> {
        let badges = matches!(
            &self.target,
            Ok(Target {
                scope: Some(SearchScope::EntireWorkspace),
                ..
            })
        );
        self.results
            .iter()
            .map(|result| ResultRow {
                result: result.clone(),
                branch: result
                    .origin
                    .filter(|_| badges)
                    .and_then(|origin| self.branch_numbers.get(&origin.branch).copied()),
            })
            .collect()
    }

    fn pattern(&self) -> &str {
        self.target.as_ref().map_or("", |t| t.pattern.as_str())
    }

    fn render_status(&self) {
        let text = match &self.target {
            Err(reason) => reason.clone(),
            Ok(target) => status_text(
                &target.pattern,
                target.scope.is_none(),
                &self.sessions.status(),
                self.results.len(),
                self.summary.as_ref(),
            ),
        };
        self.view.set_status(&text);
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

/// The folders searched, as the overlay names them.
fn folders_text(roots: &[SearchRoot]) -> String {
    let paths: Vec<String> = roots
        .iter()
        .map(|root| root.path.display().to_string())
        .collect();
    format!("In {}", paths.join(", "))
}

/// The status line: what the search is doing, or what it found.
/// `pattern` is the text matched; `in_folder` is set when the query names
/// a folder (F10).
fn status_text(
    pattern: &str,
    in_folder: bool,
    status: &SearchStatus,
    shown: usize,
    summary: Option<&SearchSummary>,
) -> String {
    if pattern.trim().is_empty() {
        return match status {
            SearchStatus::Failed(message) => Notice::from_error(message).text().to_owned(),
            _ if in_folder => "Enter opens this folder; type to search in it".to_owned(),
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
            status_text("", false, &SearchStatus::Searching, 0, None),
            "Type to search"
        );
        assert_eq!(
            status_text("dcm", false, &SearchStatus::Searching, 0, None),
            "Searching…"
        );
        assert_eq!(
            status_text("dcm", false, &SearchStatus::Searching, 3, None),
            "3 results so far…"
        );
        assert_eq!(status_text("dcm", false, &done, 0, None), "No matches");
        assert_eq!(status_text("dcm", false, &done, 1, None), "1 result");
        assert_eq!(
            status_text("dcm", false, &done, DEFAULT_RESULT_LIMIT, None),
            "The best 500 results"
        );
        assert_eq!(
            status_text("dcm", false, &done, 2, Some(&summary(1, true))),
            "2 results · 1 folder couldn't be read · only the first 200000 entries were searched"
        );
        assert_eq!(
            status_text("", true, &SearchStatus::Completed, 0, None),
            "Enter opens this folder; type to search in it"
        );
        assert_eq!(
            status_text(
                "",
                true,
                &SearchStatus::Failed("“/gone” does not exist".into()),
                0,
                None
            ),
            "“/gone” does not exist.",
            "a folder that can't be read says why"
        );
        assert_eq!(
            status_text(
                "x",
                false,
                &SearchStatus::Failed("“/gone” does not exist".into()),
                0,
                None
            ),
            "“/gone” does not exist."
        );
    }
}
