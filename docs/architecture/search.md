# Fuzzy Find: search engine

The engine behind [Fuzzy Find](../specifications/4-FUZZY-FIND.md). It has no UI yet: Phase 1 and the non-UI part of Phase 2 are built and tested, and the overlay will plug into the pieces below.

```text
 presentation (to come)     overlay: query input, results, scope selector
                                 │ opens a session, starts a search per query,
                                 │ runs the engine on a worker thread
 application  search::      roots(scope, &Workspace) ──► SearchRequest
                            SearchSessions (ids, generations, cancel tokens)
                            SearchEngine: walk once, rank per query, publish batches
                                 │ ports
                            TreeWalker            FuzzyMatcher / QueryMatcher
 infrastructure search::    LocalTreeWalker       NucleoMatcher (nucleo-matcher)
 domain       search::      SearchQuery, SearchScope, SearchResult, SearchMatch, SearchStatus …
```

## Layers

| Layer | Owns |
|---|---|
| Domain (`seshat_domain::search`) | `SearchQuery` and `CaseMode`, `SearchScope`, `SearchSessionId`, `SearchStatus`, `SearchOrigin`, `SearchResult`, `SearchMatch` (matched byte ranges). No matching library, no I/O. |
| Application ports | `FuzzyMatcher` → `QueryMatcher`, so the library can be replaced (§13); `TreeWalker` with `WalkOptions`, `Candidate`, `WalkStats`, `WalkError`. |
| Application (`seshat_application::search`) | Scopes (`roots`), ranking, `SearchEngine`, `SearchSessions`. |
| Infrastructure (`seshat_infrastructure::search`) | `NucleoMatcher`, `LocalTreeWalker`. `nucleo-matcher` is forbidden in every other layer by `scripts/check-architecture.sh`. |

## Scopes (§6)

| Scope | Searches |
|---|---|
| Current Directory | The active panel's folder, recursively. |
| Current Branch | The active branch's root folder, recursively (decision F2). Every panel of the branch is inside it. |
| Entire Workspace | Each branch's root folder, in branch order. A folder shared by several branches, or lying inside another root, is searched once. Every result keeps the branch and panel of the root it was found under (`SearchOrigin`). |
| Absolute Path | The given folder. Relative paths are refused; a missing folder fails the search with a message. |

The Trash is never searched: it is not one folder on disk.

## Walking (§12)

`LocalTreeWalker` uses `std::fs` on the worker thread. It:

- lists symbolic links but never follows them (the root may be a link to a folder, because a panel can show one);
- enters each folder once, by device and inode;
- stays on the root's filesystem (`same_device`): mount points are listed but not entered;
- skips hidden names unless asked, and the names in `ignore_names` (`.git` and `node_modules` by default; exact names only, decision F3);
- counts and skips folders it can't read, or that vanish during the walk;
- stops at `max_entries` (200,000 by default) and marks the result truncated;
- keeps native paths: a non-UTF-8 name is only lossy in its display string.

## Matching and ranking (§5)

`NucleoMatcher` wraps `nucleo-matcher` with path-aware bonuses. Every word of the query must match, accented letters match their plain form, and matching ignores case by default (`CaseMode::Insensitive`; `Smart` and `Sensitive` exist for the coming setting). fzf's operators (`^`, `$`, `'`, `!`) are not interpreted: a query is always plain text. An empty query matches nothing (§4.3).

Each candidate is matched against its **name** first and, only if the name doesn't match, against its **path relative to the searched folder**. Results are ordered by, in turn:

1. tier: an exact name (ignoring case), then any name match, then a path-only match;
2. the matcher's score;
3. the shorter relative path;
4. the path's bytes, so equal results always come out in the same order.

File metadata never affects the order. Only the best `limit` results (500 by default) are kept, in a bounded heap.

Highlights are byte ranges of whole graphemes in the displayed name, or in the relative path for a path-only match. A highlight never splits a character or separates a letter from its accent.

## Lifecycle (§11, §14)

- `SearchSessions` gives each opened Fuzzy Find a stable id. Each new query starts a new generation and cancels the previous one's `CancelToken`. `accepts(ticket)` lets only the latest generation of the open session publish. Closing the session cancels its search.
- `SearchEngine::run` walks the roots once. While walking it publishes the best results so far every 4,096 entries, then the final ranking. It caches the candidates, so later queries with the same roots and options only re-rank them. The cache lives as long as the engine (one session) and is never saved; `invalidate()` drops it. A cancelled run publishes nothing and caches nothing.

## Measured (release build, 2026-11-11)

`cargo test --release -p seshat-infrastructure --test search -- --ignored --nocapture`, 100 folders × 1,000 files:

| Step | Time |
|---|---|
| First results while walking | 2.5 ms |
| Walk and rank 100,100 entries | 85 ms |
| A new query on the cached entries | 32 ms |

## Still to build (Spec 4 Phases 2–4)

The overlay and its key binding, running the engine on a worker thread, opening and revealing results through `WorkspaceService`, the Visual Mode decision of §4.1, configuration (§15), and operations on results (§10).
