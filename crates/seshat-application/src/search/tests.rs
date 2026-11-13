//! Search tests, with an in-memory folder tree and a simple subsequence
//! matcher standing in for the real adapters.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use seshat_domain::entities::{EntryKind, Location};
use seshat_domain::search::{SearchOrigin, SearchQuery, SearchResult, SearchScope, SearchStatus};
use seshat_domain::workspace::{PanelState, Workspace};

use super::*;
use crate::operations::CancelToken;
use crate::ports::{
    Candidate, FuzzyMatcher, QueryMatcher, TreeWalker, WalkError, WalkOptions, WalkStats,
};

// Fakes

/// Case-insensitive subsequence match of the whole query (spaces removed).
/// Score: 1000 minus the gaps between matched characters.
struct Subsequence;

struct Compiled(Vec<char>);

impl FuzzyMatcher for Subsequence {
    fn compile(&self, query: &SearchQuery) -> Box<dyn QueryMatcher> {
        Box::new(Compiled(
            query
                .text
                .chars()
                .filter(|c| !c.is_whitespace())
                .flat_map(char::to_lowercase)
                .collect(),
        ))
    }
}

impl QueryMatcher for Compiled {
    fn score(&mut self, haystack: &str, ranges: Option<&mut Vec<Range<usize>>>) -> Option<u32> {
        if self.0.is_empty() {
            return None;
        }
        let mut wanted = self.0.iter().peekable();
        let mut found: Vec<Range<usize>> = Vec::new();
        let mut gaps = 0u32;
        let mut last: Option<usize> = None;
        for (i, c) in haystack.char_indices() {
            let Some(&&w) = wanted.peek() else { break };
            if c.to_lowercase().eq(std::iter::once(w)) {
                wanted.next();
                if let Some(l) = last {
                    gaps += (i - l - 1) as u32;
                }
                last = Some(i);
                let end = i + c.len_utf8();
                match found.last_mut() {
                    Some(r) if r.end == i => r.end = end,
                    _ => found.push(i..end),
                }
            }
        }
        if wanted.peek().is_some() {
            return None;
        }
        if let Some(ranges) = ranges {
            *ranges = found;
        }
        Some(1000u32.saturating_sub(gaps))
    }
}

/// An in-memory tree: every path below a root is a candidate.
#[derive(Default)]
struct FakeWalker {
    entries: Vec<(PathBuf, EntryKind)>,
    walks: AtomicUsize,
    /// Cancels this token after this many entries.
    cancel_after: Option<(usize, CancelToken)>,
}

impl FakeWalker {
    fn with(paths: &[(&str, bool)]) -> Self {
        Self {
            entries: paths
                .iter()
                .map(|(p, dir)| {
                    let kind = if *dir {
                        EntryKind::Directory
                    } else {
                        EntryKind::File
                    };
                    (PathBuf::from(p), kind)
                })
                .collect(),
            ..Self::default()
        }
    }
}

impl TreeWalker for FakeWalker {
    fn walk(
        &self,
        root: &Path,
        options: &WalkOptions,
        cancel: &CancelToken,
        found: &mut dyn FnMut(Candidate),
    ) -> Result<WalkStats, WalkError> {
        self.walks.fetch_add(1, Ordering::SeqCst);
        if !self.entries.iter().any(|(p, _)| p.starts_with(root)) {
            return Err(WalkError::NotFound(root.to_path_buf()));
        }
        let mut stats = WalkStats::default();
        let mut count = 0;
        for (path, kind) in &self.entries {
            if cancel.is_cancelled() {
                break;
            }
            if !path.starts_with(root) || path == root {
                continue;
            }
            if count == options.max_entries {
                stats.truncated = true;
                break;
            }
            found(Candidate {
                path: path.clone(),
                kind: *kind,
            });
            count += 1;
            if let Some((after, token)) = &self.cancel_after
                && count == *after
            {
                token.cancel();
            }
        }
        Ok(stats)
    }
}

fn engine(walker: FakeWalker) -> (SearchEngine, Arc<FakeWalker>) {
    let walker = Arc::new(walker);
    (
        SearchEngine::new(walker.clone(), Arc::new(Subsequence)),
        walker,
    )
}

fn request(roots: &[&str], query: &str) -> SearchRequest {
    SearchRequest {
        roots: roots
            .iter()
            .map(|r| SearchRoot {
                path: r.into(),
                origin: None,
            })
            .collect(),
        query: SearchQuery::new(query),
        options: WalkOptions::default(),
        limit: DEFAULT_RESULT_LIMIT,
    }
}

/// Runs a search, returning its summary and every published batch.
fn run(
    engine: &mut SearchEngine,
    request: &SearchRequest,
    cancel: &CancelToken,
) -> (SearchSummary, Vec<Vec<SearchResult>>) {
    let mut batches = Vec::new();
    let summary = engine.run(request, cancel, &mut |results| batches.push(results));
    (summary, batches)
}

fn names(results: &[SearchResult]) -> Vec<&str> {
    results.iter().map(|r| r.relative_path.as_str()).collect()
}

fn tree() -> FakeWalker {
    FakeWalker::with(&[
        ("/r/Documents", true),
        ("/r/Development", true),
        ("/r/Development/Code", true),
        ("/r/Development/Code/Modules", true),
        ("/r/Downloads", true),
        ("/r/Downloads/Camera", true),
        ("/r/notes.txt", false),
        ("/r/docs", true),
        ("/r/docs/docs", true),
    ])
}

// Matching and ranking (§4.3, §5)

#[test]
fn non_contiguous_queries_match_names_and_paths() {
    let (mut engine, _) = engine(tree());
    let (summary, batches) = run(&mut engine, &request(&["/r"], "dcm"), &CancelToken::new());
    assert_eq!(summary.status, SearchStatus::Completed);
    let results = batches.last().unwrap();
    // The spec's example: all three are found.
    for path in ["Documents", "Development/Code/Modules", "Downloads/Camera"] {
        assert!(
            names(results).contains(&path),
            "{path} in {:?}",
            names(results)
        );
    }
    assert!(!names(results).contains(&"notes.txt"));
}

#[test]
fn exact_names_come_first_then_name_matches_then_path_matches() {
    let (mut engine, _) = engine(tree());
    let (_, batches) = run(&mut engine, &request(&["/r"], "docs"), &CancelToken::new());
    let results = batches.last().unwrap();
    // Two exact names: the shorter path first. Then "Documents" (a name
    // match). Path-only matches come last.
    assert_eq!(names(results)[..3], ["docs", "docs/docs", "Documents"]);
    let documents = &results[2];
    assert_eq!(documents.name_match.ranges, vec![0..3, 8..9]);
    assert!(documents.path_match.is_empty());
}

#[test]
fn path_only_matches_highlight_the_path() {
    let (mut engine, _) = engine(tree());
    let (_, batches) = run(&mut engine, &request(&["/r"], "dcm"), &CancelToken::new());
    let modules = batches
        .last()
        .unwrap()
        .iter()
        .find(|r| r.relative_path == "Development/Code/Modules")
        .unwrap();
    assert!(modules.name_match.is_empty());
    let text = &modules.relative_path;
    let highlighted: String = modules
        .path_match
        .ranges
        .iter()
        .map(|r| &text[r.clone()])
        .collect();
    assert_eq!(highlighted.to_lowercase(), "dcm");
    assert_eq!(
        modules.location,
        Location::Path("/r/Development/Code/Modules".into())
    );
    assert_eq!(modules.name, "Modules");
    assert_eq!(modules.kind, EntryKind::Directory);
}

#[test]
fn ties_break_deterministically() {
    let walker = FakeWalker::with(&[
        ("/r/b/report", false),
        ("/r/a/report", false),
        ("/r/report", false),
    ]);
    let (mut engine, _) = engine(walker);
    let (_, batches) = run(
        &mut engine,
        &request(&["/r"], "report"),
        &CancelToken::new(),
    );
    assert_eq!(
        names(batches.last().unwrap()),
        ["report", "a/report", "b/report"]
    );
}

#[test]
fn results_are_bounded_to_the_best() {
    let paths: Vec<(String, bool)> = (0..100).map(|i| (format!("/r/f{i:03}"), false)).collect();
    let refs: Vec<(&str, bool)> = paths.iter().map(|(p, d)| (p.as_str(), *d)).collect();
    let (mut engine, _) = engine(FakeWalker::with(&refs));
    let mut req = request(&["/r"], "f");
    req.limit = 3;
    let (summary, batches) = run(&mut engine, &req, &CancelToken::new());
    assert_eq!(summary.candidates, 100);
    assert_eq!(names(batches.last().unwrap()), ["f000", "f001", "f002"]);
}

#[test]
fn an_empty_query_shows_nothing_but_prepares_the_candidates() {
    let (mut engine, walker) = engine(tree());
    let (summary, batches) = run(&mut engine, &request(&["/r"], "  "), &CancelToken::new());
    assert_eq!(summary.status, SearchStatus::Completed);
    assert_eq!(summary.candidates, 9);
    assert_eq!(batches, vec![Vec::<SearchResult>::new()]);
    // The first typed query reuses the walk.
    run(&mut engine, &request(&["/r"], "dcm"), &CancelToken::new());
    assert_eq!(walker.walks.load(Ordering::SeqCst), 1);
}

// Engine lifecycle (§11, §14)

#[test]
fn new_queries_reuse_the_walk_until_roots_or_options_change() {
    let (mut engine, walker) = engine(tree());
    let cancel = CancelToken::new();
    run(&mut engine, &request(&["/r"], "doc"), &cancel);
    run(&mut engine, &request(&["/r"], "dow"), &cancel);
    assert_eq!(walker.walks.load(Ordering::SeqCst), 1);

    let mut hidden = request(&["/r"], "dow");
    hidden.options.include_hidden = true;
    run(&mut engine, &hidden, &cancel);
    assert_eq!(walker.walks.load(Ordering::SeqCst), 2, "options changed");

    run(&mut engine, &request(&["/r/Development"], "dow"), &cancel);
    assert_eq!(walker.walks.load(Ordering::SeqCst), 3, "roots changed");

    engine.invalidate();
    run(&mut engine, &request(&["/r/Development"], "dow"), &cancel);
    assert_eq!(walker.walks.load(Ordering::SeqCst), 4, "invalidated");
}

#[test]
fn a_cancelled_search_publishes_nothing_and_caches_nothing() {
    let (mut engine, _) = engine(tree());
    let cancel = CancelToken::new();
    cancel.cancel();
    let (summary, batches) = run(&mut engine, &request(&["/r"], "doc"), &cancel);
    assert_eq!(summary.status, SearchStatus::Cancelled);
    assert!(batches.is_empty());

    // Cancelled half-way through the walk: the partial walk is not reused.
    let cancel = CancelToken::new();
    let mut tree = tree();
    tree.cancel_after = Some((3, cancel.clone()));
    let (mut engine, _) = super::tests::engine(tree);
    let (summary, batches) = run(&mut engine, &request(&["/r"], "doc"), &cancel);
    assert_eq!(summary.status, SearchStatus::Cancelled);
    assert!(batches.is_empty());
    let (summary, _) = run(&mut engine, &request(&["/r"], "doc"), &CancelToken::new());
    assert_eq!(summary.status, SearchStatus::Completed);
    assert_eq!(summary.candidates, 9, "walked again in full");
}

#[test]
fn results_stream_while_walking_large_trees() {
    let paths: Vec<(String, bool)> = (0..10_000)
        .map(|i| (format!("/r/file{i:05}"), false))
        .collect();
    let refs: Vec<(&str, bool)> = paths.iter().map(|(p, d)| (p.as_str(), *d)).collect();
    let (mut engine, _) = engine(FakeWalker::with(&refs));
    let mut req = request(&["/r"], "file");
    req.limit = 10;
    let (summary, batches) = run(&mut engine, &req, &CancelToken::new());
    assert_eq!(summary.candidates, 10_000);
    assert!(
        batches.len() >= 3,
        "published while walking: {}",
        batches.len()
    );
    assert!(batches.iter().all(|b| b.len() <= 10));
    assert_eq!(batches.last().unwrap()[0].relative_path, "file00000");
}

#[test]
fn walks_stop_at_the_entry_limit_and_say_so() {
    let (mut engine, _) = engine(tree());
    let mut req = request(&["/r"], "d");
    req.options.max_entries = 4;
    let (summary, _) = run(&mut engine, &req, &CancelToken::new());
    assert_eq!(summary.status, SearchStatus::Completed);
    assert!(summary.truncated);
    assert_eq!(summary.candidates, 4);
}

#[test]
fn a_missing_root_fails_only_when_nothing_else_could_be_searched() {
    let (mut engine, _) = engine(tree());
    let (summary, batches) = run(
        &mut engine,
        &request(&["/gone"], "doc"),
        &CancelToken::new(),
    );
    assert_eq!(
        summary.status,
        SearchStatus::Failed("“/gone” does not exist".into())
    );
    assert!(batches.is_empty());

    let (summary, batches) = run(
        &mut engine,
        &request(&["/gone", "/r"], "doc"),
        &CancelToken::new(),
    );
    assert_eq!(summary.status, SearchStatus::Completed);
    assert!(!batches.last().unwrap().is_empty());
}

// Scopes (§6)

fn path(p: &str) -> Location {
    Location::Path(p.into())
}

#[test]
fn current_directory_and_branch_scopes_follow_the_active_panel() {
    let mut ws = Workspace::new(path("/h"), PanelState::default());
    let home = ws.active_panel();
    let branch = ws.active_branch().id();
    let docs = ws.open_child(home, path("/h/Docs"), None).unwrap().panel;
    let projects = ws
        .open_child(docs, path("/h/Docs/Projects"), None)
        .unwrap()
        .panel;

    assert_eq!(
        roots(&SearchScope::CurrentDirectory, &ws).unwrap(),
        vec![SearchRoot {
            path: "/h/Docs/Projects".into(),
            origin: Some(SearchOrigin {
                branch,
                panel: projects
            }),
        }]
    );
    assert_eq!(
        roots(&SearchScope::CurrentBranch, &ws).unwrap(),
        vec![SearchRoot {
            path: "/h".into(),
            origin: Some(SearchOrigin {
                branch,
                panel: home
            }),
        }]
    );
}

#[test]
fn the_workspace_scope_searches_each_folder_once() {
    let mut ws = Workspace::new(path("/h"), PanelState::default());
    let home = ws.active_panel();
    let first = ws.active_branch().id();
    // A second branch through the shared Home panel.
    ws.open_child(home, path("/h/Docs"), None).unwrap();
    ws.open_child(home, path("/h/Music"), None).unwrap();
    // A root inside Home, a separate root, and the Trash.
    ws.open_root(path("/h/Docs/Deep"), PanelState::default());
    let other = ws.open_root(path("/srv"), PanelState::default());
    ws.open_root(Location::trash_root(), PanelState::default());
    assert_eq!(ws.branch_count(), 5);

    let found = roots(&SearchScope::EntireWorkspace, &ws).unwrap();
    assert_eq!(
        found,
        vec![
            SearchRoot {
                path: "/h".into(),
                origin: Some(SearchOrigin {
                    branch: first,
                    panel: home
                }),
            },
            SearchRoot {
                path: "/srv".into(),
                origin: Some(SearchOrigin {
                    branch: other.branch,
                    panel: other.panel
                }),
            },
        ]
    );
}

#[test]
fn results_carry_the_branch_and_panel_they_were_found_under() {
    let ws = Workspace::new(path("/r"), PanelState::default());
    let roots = roots(&SearchScope::CurrentDirectory, &ws).unwrap();
    let (mut engine, _) = engine(tree());
    let req = SearchRequest {
        roots: roots.clone(),
        ..request(&[], "camera")
    };
    let (_, batches) = run(&mut engine, &req, &CancelToken::new());
    let result = &batches.last().unwrap()[0];
    assert_eq!(result.origin, roots[0].origin);
    assert!(result.origin.is_some());
}

#[test]
fn the_trash_and_relative_paths_cannot_be_searched() {
    let mut ws = Workspace::new(path("/h"), PanelState::default());
    ws.open_root(Location::trash_root(), PanelState::default());
    assert_eq!(
        roots(&SearchScope::CurrentDirectory, &ws),
        Err(ScopeError::InTrash)
    );
    assert_eq!(
        roots(&SearchScope::AbsolutePath("rel/dir".into()), &ws),
        Err(ScopeError::NotAbsolute("rel/dir".into()))
    );
    assert_eq!(
        roots(&SearchScope::AbsolutePath("/abs".into()), &ws).unwrap(),
        vec![SearchRoot {
            path: "/abs".into(),
            origin: None
        }]
    );
}

// Sessions (§14)

#[test]
fn a_new_query_supersedes_the_previous_search() {
    let mut sessions = SearchSessions::default();
    assert!(sessions.search().is_none(), "no session open");
    let id = sessions.open();
    let first = sessions.search().unwrap();
    assert_eq!(sessions.status(), SearchStatus::Searching);
    let second = sessions.search().unwrap();
    assert_eq!(first.session, id);
    assert!(
        first.cancel.is_cancelled(),
        "superseded searches are stopped"
    );
    assert!(!sessions.accepts(&first), "late results are dropped");
    assert!(sessions.accepts(&second));
    assert!(!sessions.finish(&first, SearchStatus::Completed));
    assert_eq!(sessions.status(), SearchStatus::Searching);
    assert!(sessions.finish(&second, SearchStatus::Completed));
    assert_eq!(sessions.status(), SearchStatus::Completed);
}

#[test]
fn cancelling_and_closing_stop_the_running_search() {
    let mut sessions = SearchSessions::default();
    let first_id = sessions.open();
    let ticket = sessions.search().unwrap();
    sessions.cancel();
    assert!(ticket.cancel.is_cancelled());
    assert_eq!(sessions.status(), SearchStatus::Cancelled);
    // Work that finished anyway still counts as cancelled.
    sessions.finish(&ticket, SearchStatus::Completed);
    assert_eq!(sessions.status(), SearchStatus::Cancelled);

    let ticket = sessions.search().unwrap();
    sessions.close();
    assert!(ticket.cancel.is_cancelled());
    assert!(!sessions.accepts(&ticket));
    assert_eq!(sessions.current(), None);
    assert_eq!(sessions.status(), SearchStatus::Idle);
    assert_ne!(sessions.open(), first_id, "session ids are never reused");
}

// Running on the worker thread (§11, §14)

use std::sync::{Mutex, mpsc};
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(10);

fn listener() -> (SearchListener, mpsc::Receiver<SearchUpdate>) {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    (
        Box::new(move |update| {
            let _ = tx.lock().unwrap().send(update);
        }),
        rx,
    )
}

/// Updates up to and including the end of the search with `generation`.
fn until_finished(rx: &mpsc::Receiver<SearchUpdate>, generation: u64) -> Vec<SearchUpdate> {
    let mut updates = Vec::new();
    loop {
        let update = rx.recv_timeout(WAIT).expect("the search ends");
        let done = update.ticket.generation == generation
            && matches!(update.event, SearchEvent::Finished(_));
        updates.push(update);
        if done {
            return updates;
        }
    }
}

#[test]
fn the_worker_runs_searches_off_the_calling_thread() {
    let (engine, _) = engine(tree());
    let (listen, rx) = listener();
    let worker = SearchWorker::start(engine, listen);
    let mut sessions = SearchSessions::default();
    sessions.open();
    let ticket = sessions.search().unwrap();
    worker.submit(ticket.clone(), request(&["/r"], "dcm"));
    let updates = until_finished(&rx, ticket.generation);
    let SearchEvent::Results(results) = &updates[updates.len() - 2].event else {
        panic!("final results come before the end: {updates:?}");
    };
    assert_eq!(names(results)[0], "Documents");
    let SearchEvent::Finished(summary) = &updates.last().unwrap().event else {
        unreachable!()
    };
    assert_eq!(summary.status, SearchStatus::Completed);
    assert!(sessions.finish(&ticket, summary.status.clone()));
}

#[test]
fn only_the_latest_search_of_the_worker_is_shown() {
    let (engine, _) = engine(tree());
    let (listen, rx) = listener();
    let worker = SearchWorker::start(engine, listen);
    let mut sessions = SearchSessions::default();
    sessions.open();
    let first = sessions.search().unwrap();
    worker.submit(first.clone(), request(&["/r"], "dcm"));
    let second = sessions.search().unwrap();
    worker.submit(second.clone(), request(&["/r"], "notes"));
    let updates = until_finished(&rx, second.generation);
    let shown: Vec<&SearchUpdate> = updates
        .iter()
        .filter(|u| sessions.accepts(&u.ticket))
        .collect();
    assert!(
        shown
            .iter()
            .all(|u| u.ticket.generation == second.generation),
        "nothing from the superseded search is shown"
    );
    let last_results = shown
        .iter()
        .rev()
        .find_map(|u| match &u.event {
            SearchEvent::Results(results) => Some(results),
            SearchEvent::Finished(_) => None,
        })
        .expect("the latest search published results");
    assert_eq!(names(last_results), ["notes.txt"]);
}

/// Lists nothing, and only returns once cancelled; reports when it started
/// and when it stopped.
struct StuckWalker {
    events: Mutex<mpsc::Sender<&'static str>>,
}

impl TreeWalker for StuckWalker {
    fn walk(
        &self,
        _: &Path,
        _: &WalkOptions,
        cancel: &CancelToken,
        _: &mut dyn FnMut(Candidate),
    ) -> Result<WalkStats, WalkError> {
        let _ = self.events.lock().unwrap().send("started");
        while !cancel.is_cancelled() {
            std::thread::sleep(Duration::from_millis(1));
        }
        let _ = self.events.lock().unwrap().send("stopped");
        Ok(WalkStats::default())
    }
}

#[test]
fn dropping_the_worker_stops_its_search_without_waiting_for_it() {
    let (events_tx, events) = mpsc::channel();
    let walker = StuckWalker {
        events: Mutex::new(events_tx),
    };
    let engine = SearchEngine::new(Arc::new(walker), Arc::new(Subsequence));
    let (listen, rx) = listener();
    let worker = SearchWorker::start(engine, listen);
    let mut sessions = SearchSessions::default();
    sessions.open();
    let ticket = sessions.search().unwrap();
    worker.submit(ticket.clone(), request(&["/r"], "x"));
    assert_eq!(events.recv_timeout(WAIT), Ok("started"));
    drop(worker); // returns at once; the walk is still running
    assert_eq!(
        events.recv_timeout(WAIT),
        Ok("stopped"),
        "and it is cancelled"
    );
    assert!(ticket.cancel.is_cancelled());
    let updates = until_finished(&rx, ticket.generation);
    let SearchEvent::Finished(summary) = &updates.last().unwrap().event else {
        unreachable!()
    };
    assert_eq!(summary.status, SearchStatus::Cancelled);
}
