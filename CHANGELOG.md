# Changelog

## Unreleased: spatial workspace

- **Context-aware navigation** ([ADR 0006](docs/decisions/0006-context-aware-navigation.md)): `l`, like `Enter`, opens the folder under the cursor, and Seshat decides whether that continues the current branch, returns to the branch that already has that folder (where you left it, with its panels and state), or starts a new branch. Nothing is duplicated. The plain move to the next panel is now `L`. Folders renamed in place are followed; a folder replaced under the same name starts fresh. Sidebar locations reuse their existing branch. The saved workspace is format 2; older files still load. `/` now replaces a panel's remembered search instead of appending to it.
- **UI fixes:** every panel header now has the same height, whether or not it shows a branch badge, a fork button or Focus Mode arrows. Moving to a panel scrolls it fully into view, including the rightmost panel and panels that widen as they become active, and the view no longer drifts when keyboard focus moves or the layout changes.
- **Software centre metadata:** the developer (Kelvin Morris) is named, and the description now covers the spatial workspace.
- **Example theme:** [Catppuccin Mocha](docs/themes/catppuccin-mocha.toml), installed with the package as `share/doc/seshat/examples/themes/catppuccin-mocha.toml`.
- **Licence:** GPL-3.0-or-later confirmed; the full text is in `LICENSE` and installed with the package (`share/licenses/seshat`).
- **Performance run** ([results](docs/review/performance.md)): all approved targets met at 50 branches and 211 panels. Saving the workspace no longer blocks the UI (the write and its `fsync` run on a writer thread). Opt-in benchmarks and `scripts/ui/workspace-perf.sh` for reruns.
- **Command-line completion:** Tab completes command names, folder paths for `:copy`, `:move` and `:open` (with `~`, relative paths and `..`), and branch targets such as `b2`. Repeated Tab and Shift+Tab cycle through several matches, which are listed in the status bar.
- **Branches side by side** ([ADR 0005](docs/decisions/0005-branch-layout.md)): tree depth across, branches down. Shared folders span their branches; connectors link folders to subfolders, with the active path in the accent colour. Other branches are compact cards. Short windows fall back to one row with "+N branches" stubs; `layout = "strip"` keeps the single strip.
Implements the [spatial file manager specification](docs/specifications/1-SPATIAL-FILE-MANAGER.md) on top of 0.1.0. Product decisions D1–D19 are in [DECISIONS.md](docs/architecture/DECISIONS.md) and the architecture in [ADR 0004](docs/decisions/0004-spatial-workspace.md).

- **Workspace:** folders open as persistent panels in a horizontally scrolling strip. Each panel keeps its cursor, scroll position, sort order, view, search and selection.
- **Branches:** opening another folder from a panel that already has a child starts a new branch; shared folders exist once. Switch with `[` and `]`, the branch bar or `:branch`; close branches without touching files.
- **Focus Mode** (`zz` or the toolbar button) expands the active panel; navigation and branch switching keep working.
- **Vim-style keys:** Normal, Visual (`v`, `V`) and Command (`:`) modes. Bindings are configurable in `settings.toml`, and `:help` lists them.
- **Operations across panels and branches:** the workspace-wide selection is copied or moved to an explicit destination (paste into a panel, `:copy`/`:move`, the destination picker, or drag and drop), with a confirmation when the sources come from several panels. The operation manager and the D-Bus protocol are unchanged.
- **The workspace is saved** to `$XDG_STATE_HOME/seshat/workspace.toml` (versioned) and restored on launch.
- **Behaviour changes:**
  - Typing letters no longer starts a search; use `/` or Ctrl+F, or set `classic_keys = true` (D1).
  - Back and Forward return to previously active panels (D2).
  - Sidebar and address-bar locations open as new branches (D3).
  - A deleted folder or removed device leaves its panels in place, marked unavailable, instead of returning Home (D14).
  - Selections never include items hidden by a search filter.
- **Tests:** 295 Rust tests (from 165) and a second sandboxed UI test, `scripts/ui/workspace.sh`, which walks the spec's definition of done.

## 0.1.0 (unreleased)

First complete version, built in six phases (see `docs/specifications/0-INITIAL-PRODUCT-SPEC.md` §17).

- **Foundation:** GTK4 + Relm4 ([ADR 0001](docs/decisions/0001-ui-framework.md)), Clean Architecture workspace, native Wayland verification.
- **Filesystem:** XDG locations, async GIO listing, navigation history, breadcrumbs, address bar, live updates, Trash browsing, removable and network devices.
- **UI and theming:**
  - list and grid views, natural sorting, search, multi-selection, context menus, drag-out
  - light, dark and validated custom TOML themes
  - preferences saved between sessions
- **File operations:**
  - a central Operation Manager handling copy, move, rename, delete, Trash, restore and create
  - progress, cancellation, conflict resolution and partial-failure reporting
  - atomic overwrites; work continues after the window closes
  - clipboard interoperability with GNOME and KDE file managers, and drop-in
- **D-Bus IPC:** a typed, versioned protocol with sequence numbers and snapshots; the independent example client `seshat-progress` ([ADR 0002](docs/decisions/0002-ipc.md)).
- **Release work:**
  - a Nix package with desktop entry, icon and AppStream metadata ([ADR 0003](docs/decisions/0003-packaging.md))
  - security, performance and architecture reviews with fixes ([docs/review](docs/review))
  - automated architecture checks and a single `scripts/check.sh`
