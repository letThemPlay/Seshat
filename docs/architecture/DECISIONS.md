# Architecture decisions

Full ADRs live in [`docs/decisions/`](../decisions):

| ADR | Topic |
|---|---|
| [0001](../decisions/0001-ui-framework.md) | UI framework: GTK4 + gtk4-rs + Relm4, no libadwaita |
| [0002](../decisions/0002-ipc.md) | D-Bus IPC with zbus |
| [0003](../decisions/0003-packaging.md) | Nix package first |
| [0004](../decisions/0004-spatial-workspace.md) | Spatial workspace: panel tree, branches, effects-based workspace service |
| [0005](../decisions/0005-branch-layout.md) | Branches side by side with connectors |
| [0006](../decisions/0006-context-aware-navigation.md) | Context-aware navigation: continue, reactivate or create |
| [0007](../decisions/0007-backward-branch-pruning.md) | Backward branch pruning (`db`, `dB`) and workspace undo |

## Spatial file manager: product decisions (approved 2026-11-09)

These resolve conflicts and gaps between [the original specification](../specifications/0-INITIAL-PRODUCT-SPEC.md) and [the spatial file manager specification](../specifications/1-SPATIAL-FILE-MANAGER.md). Every one was approved by the maintainer before implementation. No requirement of the original specification was removed.

| # | Topic | Decision |
|---|---|---|
| D1 | Type-to-search vs Normal Mode keys | Search starts with `/` or Ctrl+F. Typing printable characters no longer starts a search while the workspace handles keys. A "classic keys" preference restores type-to-search and disables single-letter bindings. |
| D2 | Back/Forward (original §5.1) | A workspace **jump list** of panel activations: Alt+Left/Right, mouse back/forward buttons, Ctrl+O/Ctrl+I. It never creates or destroys panels and skips closed ones. |
| D3 | Navigating to non-child locations (sidebar, address bar, device mount, themes folder) | If the location is a panel on the active branch, activate it. Otherwise open it as a **new root branch**; it is never attached to the active panel. A breadcrumb ancestor activates its panel; if it has none, a parent panel is inserted above the root. Backspace at a root does the same. |
| D4 | One root or many | The workspace is a forest with ordered roots. The default workspace is a single Home root. |
| D5 | Closing the last branch | Refused with a notice. |
| D6 | Opening a child several branches pass through | Activates the most recently active branch containing it. |
| D7 | Selection scope for operations | Operations act on the workspace-wide selection. When it spans more than one panel, a confirmation lists sources grouped by panel, the destination and the operation. Trash/Delete always confirm when the selection spans panels. Rename requires exactly one item. |
| D8 | Visual Mode | Survives `h`/`l`. Esc leaves Visual Mode and keeps the selection; Esc in Normal Mode clears it. A plain click replaces the selection within that panel only; Ctrl/Shift clicks work as before. |
| D9 | Yank register vs clipboard | One mechanism: the system clipboard (interoperable with GNOME/KDE). Ctrl+C/X/V and `y`/`x`/`p` use it. A status indicator shows what is staged and whether it copies or moves. |
| D10 | `:copy`/`:move` destination syntax | `:copy <path>`, `:copy b<N>` (active panel of branch N), or no argument for a destination picker. |
| D11 | Global vs per-panel view preferences | Global preferences are the defaults for new root panels. New children inherit their parent's settings. Changing a setting affects that panel and also updates the saved default. |
| D12 | Expanded metadata, column configuration, preview state | Not built in this feature (the features do not exist yet). Marked N/A in status; the persistence schema leaves room for them. |
| D13 | Listing memory vs "no reloads" | Listings are cached for the 16 most recently used panels; only cached panels are watched. An evicted panel reloads when activated; its logical state is never lost. |
| D14 | Folder deleted / device removed while open | The panel and its descendants stay, marked **Unavailable**, with actions to close the branch or go to the nearest existing ancestor. No branch is silently destroyed. Ejecting releases every watch on that device. External renames are not followed. |
| D15 | Same-named sources from different folders in one copy | Handled by the existing conflict resolver; covered by tests. No manager change. |
| D16 | Workspace persistence | `$XDG_STATE_HOME/seshat/workspace.toml`, versioned, written atomically, restored on launch by default, with a preference for a clean start. |
| D17 | Grid view and `h`/`l` | `h`/`l` always move between panels; arrow keys move within the grid. |
| D18 | Documentation names | `CLAUDE.md`, `1-SPATIAL-FILE-MANAGER.md`; this document set created alongside the existing docs. |
| D19 | View value types | `SortKey`, `SortOrder`, `ViewMode` and `ViewOptions` moved to the domain and re-exported from `seshat_application::view`. |

## Branch layout decisions (approved 2026-11-09)

Side-by-side branch rendering, [ADR 0005](../decisions/0005-branch-layout.md).

| # | Topic | Decision |
|---|---|---|
| L1 | What inactive branches show | Compact cards drawn from saved panel state: folder, cursor item, selection count, branch number. No extra folder reads; the listing cache limit (D13) holds. |
| L2 | Default layout | Side by side by default, falling back to a single strip when the window is too short for any lane. `[workspace] layout = "strip"` keeps the strip. |
| L3 | Row order | Fixed `[`/`]` order. Rows stay put while the active branch is in view. |
| L4 | Many branches | At most six branch rows, fewer in short windows. The rest collapse into "+N branches" stubs that open the branch overview. |

## Command-line completion decisions (approved 2026-11-09)

| # | Topic | Decision |
|---|---|---|
| P1 | Several matches | The first Tab fills in the common prefix and lists the matches in the status bar; further Tab and Shift+Tab presses cycle through them. |
| P2 | Which entries | Folders only (links to folders count), since `:copy`, `:move` and `:open` take a folder. Dot folders appear only after a typed `.`. |
| P3 | Case | Smartcase: an all-lowercase prefix matches any case; a capital letter makes matching exact. |
| P4 | Branch targets | `b` + Tab lists `b1 …`, `b2 …` for `:copy` and `:move`. |

## Performance run decisions (approved 2026-11-09)

Results in [review/performance.md](../review/performance.md).

| # | Topic | Decision |
|---|---|---|
| R1 | Targets (release build, development machine) | Screen update p95 < 16 ms with 50 branches and 200 panels; branch switch < 50 ms; Focus Mode toggle < 16 ms; layout < 1 ms; workspace save < 20 ms on the UI thread; no UI freeze over 100 ms except loading a 100,000-entry folder; no folder reads on Focus Mode or branch switches. |
| R2 | Scale | 50 branches, about 200 panels, folders of 1,000 entries, and one of 100,000. |
| R3 | 100,000-row model swap (~250 ms) | Exempt and documented as a follow-up (incremental model updates). |
| R4 | Timing in CI | None: benchmarks are opt-in ignored tests and a UI script run by hand. CI keeps the deterministic counter checks. |
| R5 | The run's two open findings | Deferred (2026-11-09): repeated folder reads beyond the 16-listing cache (D13 stays as is) and the pauses during live reloads of 100,000-entry folders are documented, to be revisited only if they become a problem. |

## Licence (approved 2026-11-09)

| Topic | Decision |
|---|---|
| Licence | GPL-3.0-or-later, as already declared in `Cargo.toml`, `flake.nix` and the AppStream metadata. The full text is in `LICENSE` and installed with the package. |
| Copyright notice | "Copyright (C) 2026 the Seshat contributors", in the README. No per-file headers. |

## Project metadata (approved 2026-11-09)

| Topic | Decision |
|---|---|
| Developer | Kelvin Morris, AppStream developer id `dev.kdmorris` (matching the app id). |
| Description | The AppStream description covers the spatial workspace: panels, side-by-side branches, Focus Mode, Vim-style keys and the command line. |
| App id | `dev.kdmorris.Seshat` stays, despite AppStream's pedantic note about capitals: the desktop file, icon and D-Bus name all use it. |

## Context-aware navigation decisions (approved 2026-11-10)

[ADR 0006](../decisions/0006-context-aware-navigation.md) and the [specification](../specifications/2-CONTEXT-AWARE-NAVIGATION.md).

| # | Topic | Decision |
|---|---|---|
| C1 | Where reactivation lands | On the reactivated branch's remembered panel when it lies at or below the opened folder, otherwise on the folder itself. |
| C2 | What `l` means | `l` opens the folder under the cursor, exactly like `Enter`. The plain move to the next panel stays as `panel-right` on `L`. With nothing under the cursor, `l` moves to the next panel. The spatial specification §4.1 and §7.1 note this. |
| C3 | Folder identity | Device and inode, recorded per panel. Matching stays by location within the parent folder; a changed identity at the same name resets the panel's state and marks its descendants unavailable; the same identity under a new name is followed. Symbolic links keep their own identity. |
| C4 | Folders open elsewhere | Only children of the current panel are matched; distinct navigation contexts are never merged. |
| C5 | Sidebar and address bar | Reuse an existing root panel for the same location (its branch comes back where it was left) before creating a new root. Refines D3. |
| C6 | Visual Mode | `l` and `Enter` navigate into folders, but never launch files; a hint says to press Esc first. |
| C7 | Branch history | A branch's history is its path, its remembered panel and each panel's state; the global Back/Forward (D2) is unchanged. |

## Backward branch pruning decisions (approved 2026-11-11)

[ADR 0007](../decisions/0007-backward-branch-pruning.md) and the [specification](../specifications/3-BACKWARD-BRANCH-PRUNING.md).

| # | Topic | Decision |
|---|---|---|
| B1 | Navigation segments | `db` goes back one step: the active panel's parent. `dB` goes back one branch segment: the nearest ancestor shared with another branch, or the branch's root. Both are derived from the tree, never from names, positions or counts. On a path without forks the spec's §4/§5 examples therefore don't apply as written; the spec calls them illustrations. |
| B2 | Where pruning starts | Only from the active branch's tip. With panels beyond the active panel, or other branches through it, nothing is pruned and the status bar says why. Removed panels are exactly those from the destination (kept) to the active panel, so shared ancestors are never removed. |
| B3 | Pruning to a shared ancestor | The active branch ends. The most recently active branch through the destination takes over there; every other branch keeps its panels, ids and state. |
| B4 | Focus Mode flag | Pruning and undo never change it, whatever the "keep Focus Mode when switching branches" preference says: a branch ending is not a branch switch. |
| B5 | Workspace undo | `u` puts the removed panels back with their ids and state, at their old place, and makes the pruned branch active again. Refused, changing nothing, if the workspace changed there since (destination gone, folder reopened, branch grown or closed); such a change is dropped from the history. 20 changes, this session only, not saved. Undo leaves Focus Mode and the jump list alone and never undoes file operations; not bound to Ctrl+Z. |
| B6 | Selections and Visual Mode | Selections in removed panels go with them; the status bar gives the count and undo restores them. In Visual Mode `db`/`dB` do nothing and say so; `d` never deletes files. |
| B7 | Keys | `prune-back` = `db`, `prune-back-segment` = `dB`, `undo-workspace` = `u`, configurable. Incomplete or invalid sequences are cancelled and swallow their second key. A pending `d` is dropped on pointer actions in the workspace, focus moving to another panel, and the window losing focus. |
| B8 | Override precedence | The defaults of the three new actions give way to an overlapping user binding (dropped with a warning) instead of rejecting it, so existing `d` or `u` bindings keep working. Other conflicts are handled as before. |
| B9 | Focus Mode | `db`, `dB` and `u` are not available in Focus Mode: nothing changes and the status bar says so. This departs from spec §11, which asked for pruning to work in Focus Mode (product decision, 2026-11-11). |

## File action rules (2026-11-11)

From the design review (R1): the rules for file actions moved from the window to `seshat_application::workspace::file_actions`. Behaviour is unchanged except for these deliberate changes. A1 and the copy and cut part of A2 were approved with the plan (2026-11-11); the Move to Trash part of A2 applies the same rule and is awaiting review in the pull request.

| # | Topic | Decision |
|---|---|---|
| A1 | Renaming an item in the Trash | The Rename dialog and `:rename <name>` now give the same answer: "Items in the Trash can't be renamed." Before, the dialog did nothing silently. Two selected items in one panel now also say "Select exactly one item to rename." instead of doing nothing. |
| A2 | Trash items in copy, cut and Move to Trash | Items in the Trash can't take part, so they are left out, but no longer silently: the status bar says "N items in the Trash were left out." A selection made only of Trash items is refused with a reason ("Items in the Trash can only be restored or deleted.", or for Move to Trash "These items are already in the Trash."). Restore is unchanged. |

## Fuzzy Find search engine (2026-11-11)

[Specification](../specifications/4-FUZZY-FIND.md) and [search.md](search.md). The engine is built without its UI (Phase 1 and the non-UI part of Phase 2).

| # | Topic | Decision |
|---|---|---|
| F1 | Matching library | `nucleo-matcher` (the matching core of nucleo, Spec 4 §5), behind the `FuzzyMatcher` port in infrastructure only. Its licence, MPL-2.0, was added to `deny.toml`. It is file-level copyleft and compatible with GPL-3.0-or-later. Approved with the plan, 2026-11-11. The high-level `nucleo` crate was not used: it brings its own thread pool, while the engine runs on a worker thread the shell controls, stopped by a `CancelToken` like file operations. |
| F2 | "Current Branch" scope | The active branch's root folder, searched recursively. Every panel of a branch lies inside it. Bounded by `max_entries`, `same_device` and cancellation. Approved 2026-11-11. |
| F3 | Ignore rules | Exact names (`.git` and `node_modules` by default) until configuration (Spec 4 Phase 4), so no glob dependency is added yet. |
| F4 | Query syntax and case | Queries are plain text: fzf's operators (`^`, `$`, `'`, `!`) are not interpreted, so a name with those characters is found as typed. Matching ignores case by default (Spec 4 §4.3); `Smart` and `Sensitive` are ready for the setting in §15. |

## Status messages (approved 2026-11-11)

From the design review (R5). Status-bar messages are typed `Notice` values formatted in one place, `seshat-presentation/src/notice.rs`.

| # | Topic | Decision |
|---|---|---|
| N1 | Severity | Two kinds. **Error** (refusals and failures, including a theme that can't be applied) is drawn in the theme's error colour. **Info** (confirmations and information) uses the normal status text colour. Before, every message used the warning colour. Both come from existing theme tokens, so custom themes need no change. |
| N2 | Screen readers | Error messages are announced through GTK's accessible announcements, at high priority, once each time they are raised. Information is not announced, so routine work stays quiet. |
| N3 | Wording | Every message is a sentence: capital first letter, closing full stop unless it already ends in `.`, `!`, `?` or `…`. Lists of completion matches are shown as they are. Counts use one helper ("1 item", "3 items"), so "has 1 issue(s)" now reads "has 1 issue". The Visual Mode refusal names the action instead of always saying "Pruning". |
