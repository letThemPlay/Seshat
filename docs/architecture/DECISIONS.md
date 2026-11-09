# Architecture decisions

Full ADRs live in [`docs/decisions/`](../decisions):

| ADR | Topic |
|---|---|
| [0001](../decisions/0001-ui-framework.md) | UI framework: GTK4 + gtk4-rs + Relm4, no libadwaita |
| [0002](../decisions/0002-ipc.md) | D-Bus IPC with zbus |
| [0003](../decisions/0003-packaging.md) | Nix package first |
| [0004](../decisions/0004-spatial-workspace.md) | Spatial workspace: panel tree, branches, effects-based workspace service |
| [0005](../decisions/0005-branch-layout.md) | Branches side by side with connectors |

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
