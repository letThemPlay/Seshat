# ADR 0008: Opening Fuzzy Find results in the workspace

- **Status:** Accepted
- **Date:** 2026-11-12
- **Spec references:** [Fuzzy Find](../specifications/4-FUZZY-FIND.md) §9; decisions F5–F9 in [DECISIONS.md](../architecture/DECISIONS.md); builds on [ADR 0004](0004-spatial-workspace.md) and [ADR 0006](0006-context-aware-navigation.md)

## Context

Fuzzy Find searches a folder recursively, so most results lie several levels below the panel that was searched. The specification asks that opening one uses "the existing spatial navigation system" and "creates or activates the appropriate panel and branch according to existing workspace rules", without destroying or duplicating anything (§9.1). It doesn't say how a result that isn't a child of any panel fits into the panel tree.

The workspace has two ways in:
- `Workspace::open_directory` opens a **child** of a panel, choosing between continuing the active branch, reactivating another branch and starting a new one (ADR 0006, C1–C4).
- `WorkspaceService::open_location` opens a location from **outside** the tree: the active branch's panel for it, an existing root for it, or a new root branch (D3, C5).

The panel tree requires every panel to be a child of the panel for its parent folder, so a deep result can't simply be attached to the panel that was searched.

## Decision

### Open along the path (F5)

A folder result is opened **one folder at a time** from its origin panel: the panel that was searched, recorded in the result's `SearchOrigin`. `Workspace::open_path(from, steps)` applies `open_directory` to each folder between the origin and the result. So:

- panels that already exist on the way are reused, never duplicated;
- reaching a folder that another branch shows reactivates that branch, as `l` would;
- a new branch starts only where continuing would replace an existing path, so at most one is created;
- each parent's cursor is left on the folder opened below it, as if `l` had been pressed at every level;
- the walk **ends on the folder asked for**. Where the last step brings back another branch, `l` would land on that branch's remembered panel when it lies deeper (C1); `open_path` activates the requested folder instead, and the deeper panels stay on the branch. Otherwise opening `Downloads` could leave `Downloads/Camera` active, and Reveal couldn't put the cursor in the folder it opened (§9.1, §9.4). Found by the UI test, 2026-11-12;
- the whole action is **one** entry in the Back/Forward jump list (D2).

Apart from where it ends, the result is exactly what the user would get by pressing `l` on each folder in turn, which keeps it predictable and needs no new workspace rules.

### Files are launched; the workspace is unchanged (§9.2)

A file result is opened in its default application with the existing `Effect::Launch`. Panels, branches and the active panel stay as they were.

### Reveal moves the cursor, not the selection (F9)

Revealing a result opens its parent folder along the path, then moves that panel's cursor to the result once it is listed (`cursor_when_listed`). The selection is left alone. `select_when_listed`, used after creating or renaming an item, still selects it as before; both share one mechanism, `PendingCursor`.

### Without a usable origin, the address bar's rule (D3)

A result with no origin panel, because it came from a typed absolute path (F10), or because the panel was closed in the meantime, or because it doesn't lie below that panel's folder, opens through `open_location`, exactly like a location typed in the address bar.

### Not supported: forcing a new branch (F8)

§9.3's "open in a new branch" is optional. Starting a branch from a panel that is the last of its branch would leave a branch ending in a panel with children, which the model's invariants forbid. Enter already starts a new branch wherever continuing would replace a path.

## Consequences

- Opening, revealing and launching results go through `WorkspaceService` (`workspace/service/found.rs`) and the workspace's own navigation. Fuzzy Find keeps no model of panels or branches.
- Deep results add one panel per level, as navigating with the keyboard would. The listing cache (D13) keeps only the visible panels loaded.
- `open_path` is part of the randomised invariant test, alongside every other workspace operation.
- Fuzzy Find never prunes, closes branches or removes panels.
