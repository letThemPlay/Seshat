# ADR 0006: Context-aware navigation

- **Status:** Accepted
- **Date:** 2026-11-10
- **Spec references:** [context-aware navigation](../specifications/2-CONTEXT-AWARE-NAVIGATION.md); decisions C1–C7 in [DECISIONS.md](../architecture/DECISIONS.md)

## Context

The new specification makes `l` (and `Enter`, an exact alias) open the folder under the cursor. It asks the application to decide whether the user is continuing the active branch, returning to a branch that already has that folder, or starting a new one, and to never duplicate panels or branches. It refines the spatial model of [ADR 0004](0004-spatial-workspace.md) rather than replacing it.

Most of what it needs already existed:
- Branches are first-class and derived from the panel tree, so shared ancestors exist once.
- Opening a child activated an existing child instead of duplicating it.
- Siblings could never show the same location.

What was missing:
- An explicit, testable resolution.
- Restoring a reactivated branch where it was left.
- A notion of folder identity beyond the path.
- Reuse of root branches opened from the sidebar.
- The key binding.

## Decision

### Resolution in the domain

`Workspace::open_directory(from, location, identity, state)` resolves opening a folder from panel `from` and returns an `Opened` with an explicit `Resolution`:

```
child of `from` for the folder (by location; see identity below)?
  yes, on the active branch's path   → ContinueActiveBranch: activate it, keep what is below it
  yes, only on other branches        → ReactivateExistingBranch: the most recently active branch
                                       through it, landing on its remembered panel if that is at
                                       or below the folder (C1), else on the folder
  no, and `from` is the branch's tip → ContinueActiveBranch: the branch grows
  no, otherwise                      → CreateNewBranch from `from`
```

`Opened` also carries the folder's panel, the panel the user lands on, the active branch and the panels whose folder changed under them.

Further rules:
- **Scope:** only children of `from` are considered (C4), so distinct navigation contexts are never merged.
- **Sidebar and address bar** (`open_root`): reuse an existing root panel for the same location, whose branch comes back where it was left, before creating a new root (C5).
- **Invariants:** the existing checks still hold after every operation, and the randomised tests now include identity-based opens, renames and replacements.

### Folder identity (C3)

Listings carry each entry's device and inode (`DirectoryIdentity`), which the GIO reader takes from `unix::device` and `unix::inode`. Listings never follow symbolic links, so a link has its own identity. Panels remember the identity of their folder. Matching stays by location within the parent, with two refinements:
- **Same name, different identity:** the folder was replaced. The panel's view settings stay, but its cursor, scroll position and selection are dropped, and the panels below it are marked unavailable.
- **Same identity, different name:** the folder was renamed in place. The panel and everything below it, including cursors, selections and scroll anchors, follow the new path.
- **Unknown identity:** when either identity is unknown (GVfs locations, older saved workspaces), location alone decides, and a newly reported identity is simply recorded.

### One entry point in the application

`WorkspaceService::open_entry` is the only way anything is opened: `l`, `Enter`, double-click and the command line all call it.
- **Folders** go to `open_directory`, which returns the resolution. The resolution is logged at debug level (`navigation resolved resolution=…`).
- **Files** produce a launch effect.

Panels whose folder was renamed or replaced lose their cached listing and watch, and are read again. Reactivating a branch reads and watches nothing new while its listings are cached. Panels marked unavailable wait for Retry instead of being read when they come back into view.

### Keys and modes

- **Bindings:** `l` and `Enter` bind to the `open` action. The previous meaning of `l`, moving to the next existing panel without opening anything, stays as `panel-right` on `L` (C2). The spatial specification's §4.1 and §7.1 carry a note saying so.
- **Visual Mode:** the mode machine turns `open` into `Intent::Open { launch_files }`. In Visual Mode folders still navigate (needed to select across panels), but files do not open (C6).
- **Empty folder:** with nothing under the cursor, `l` moves to the next panel.

### Persistence

The workspace file is format version 2: each panel may carry `identity = "device:inode"`, stored as a string because TOML integers cannot hold every 64-bit value. Version 1 files load unchanged, with identities unknown, and no duplicate branches are created on restore.

## Consequences

- **Testing:** navigation behaviour is decided in one place, in the domain, and is tested there and in the application service, with tests named after the specification (`nav_16_2_*`). The workspace UI test reactivates a branch from Home, checks that `Enter` behaves exactly like `l`, and confirms no panel or branch is duplicated.
- **Unchanged:** Focus Mode stays presentation only, branch order stays depth-first, and the global Back/Forward history (D2) stays unchanged (C7).
- **Not followed:** a folder renamed into a different parent folder. It is found by its path only, as before.
