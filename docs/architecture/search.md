# Fuzzy Find

[Fuzzy Find](../specifications/4-FUZZY-FIND.md): the engine, the worker thread that runs it, the overlay (`Ctrl+P`) and opening results in the workspace. The Current Directory scope is reachable from the app; the scope selector, reveal and configuration are still to come.

```text
 presentation  FuzzyFindController (app/fuzzy_find.rs) + FuzzyView (overlay)
                                 │ opens a session, starts a search per query on
                                 │ the SearchWorker; Enter → WorkspaceService::open_result
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

## Running in the background (§11)

`SearchWorker` runs one session's searches on a thread of its own (`seshat-search`), owning that session's `SearchEngine` and its cache. It keeps at most one search waiting: a newer submission replaces one that hasn't started, and the caller cancels the running one through its ticket. Updates (`SearchEvent::Results`, then `SearchEvent::Finished`) go to a listener on the worker thread; the caller drops those its `SearchSessions` no longer accepts. Dropping the worker cancels its searches without waiting for the thread, so a walk stuck on a slow mount can't hold up closing Fuzzy Find or quitting. A panic in a search ends it as failed instead of taking the app down.

## Opening results (§9)

Opening goes through the workspace's own navigation ([ADR 0008](../decisions/0008-opening-search-results.md)), in `WorkspaceService` (`workspace/service/found.rs`):

- `open_result`: a folder (or a link to one) is opened along the path from the panel that was searched, reusing existing panels (F5); a file is launched and the workspace is left as it is.
- `reveal_result`: opens the folder the result is in, the same way, and moves the cursor to the result without changing the selection (F9).
- Without a usable origin panel, both use the address bar's rule (D3).

## The overlay (§4, §7, §8)

`Ctrl+P` (the `fuzzy-find` action, which gives way to a user binding of the same keys, B8) opens a compact panel over the workspace. It is refused in Visual Mode (F6). The workspace doesn't change while it is open (F12).

- **Opening** starts a `SearchWorker` and runs an empty query at once for the chosen scope, so its folders are walked while the user starts typing.
- **Scopes** (§6): a drop-down offers Current Directory, Current Branch and Entire Workspace, also picked with `Alt+1`, `Alt+2` and `Alt+3` from the search field; the line beside it names the folders searched (`roots`). The choice lasts until Seshat closes and isn't saved (F12); the saved default comes with configuration. The drop-down never takes keyboard focus, so the window's key bindings can't act on the workspace while the overlay is open. With Entire Workspace, each result shows a badge with the number of the branch it was found in, as the branch bar numbers them (§6.3); a folder shared by several branches is searched once and its results carry the first of them.
- **Typing** starts a new search for every change (the search field waits 50 ms); the previous search is superseded through its ticket, and results that arrive for it are dropped.
- **The highlight** follows the result, not its position: when results update, the same result stays highlighted if it is still there, otherwise the best one.
- **Keys** stay in the search field: `Up`, `Down`, `Page Up` and `Page Down` move the highlight, `Enter` opens, `Alt+Enter` reveals (F7), `Esc` closes, and `Ctrl+Backspace` deletes a word as in any text field. A click highlights and a double-click opens; keyboard focus returns to the search field.
- **Reveal** (`Alt+Enter`, §9.4) returns the result for `WorkspaceService::reveal_result`: its folder opens along the path, bringing back its branch if needed, and the cursor moves to the result once listed; the selection is left alone (F9).
- **A typed folder** (§6.4, F10): a query starting with `/`, `~/` or exactly `~` is read by `PathQuery` (application): the text up to the last `/` is the folder, with the address bar's rules (`parse_address`), and the rest is the pattern. The folder is searched as the `AbsolutePath` scope and the drop-down is greyed out. With nothing after the folder, `Enter` opens it as the address bar would (D3); a folder that doesn't exist or can't be read is reported on the status line and `Enter` does nothing. Results from a typed folder have no origin panel, so they open through `open_location` too (ADR 0008).
- **Closing**, by `Esc`, by opening a result or by focus moving elsewhere, drops the worker, which cancels its search.
- **Rows** show an icon and, for folders, a trailing `/`, so the kind is never shown by colour alone (§16). Matched characters are bold and underlined (`fuzzy_view::highlighted`, escaped Pango markup on character boundaries). The second line says which folder the result is in, or shows the path with its matches when only the path matched. The highlighted row has an accent bar as well as the selection colour. Screen readers hear "name, folder, in parent".
- **The status line** says what the search is doing: "Type to search", "Searching…", "N results so far…", the result count or "No matches", folders that couldn't be read, and whether the walk stopped at its limit.

## Still to build (Spec 4 Phase 4)

Configuration (§15, F11) and timings with the overlay in a release build. Operations on several results (§10, F13) come in a later phase.
