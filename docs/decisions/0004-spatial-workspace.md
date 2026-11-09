# ADR 0004: Spatial workspace architecture

- **Status:** Accepted
- **Date:** 2026-11-09
- **Spec references:** [spatial file manager](../specifications/1-SPATIAL-FILE-MANAGER.md) §3, §5, §6, §7, §8, §12, §13; product decisions D1–D19 in [DECISIONS.md](../architecture/DECISIONS.md)

## Context

The spatial specification turns the single-pane explorer into a workspace of persistent directory panels with first-class branches, Focus Mode, Vim-style modes and operations across panels. It supplements the original specification, which stays in force: Clean Architecture, the central operation manager, the D-Bus protocol and native Wayland.

In the proof of concept, all per-view state (history, listing, view options, load generations, the watch) lived in the `App` component. The selection lived in the GTK `MultiSelection`. Neither could survive widgets being disposed and recreated.

## Decision

### Domain: a forest of panels with derived branches

`seshat_domain::workspace::Workspace` holds panels in a forest (`roots` plus parent and child links) and a set of `Branch`es.

- **A branch is a leaf.** Each branch records its `tip` (a leaf panel) and a `cursor` (the panel it is at). Its path is computed from parent links, so a shared ancestor exists once however many branches pass through it. The invariant is that leaves and branches correspond one to one.
- **Opening a child:** from a branch tip it extends that branch; from a panel that already has children it starts a new branch. An existing child for the same location is activated, never duplicated.
- **Closing a branch** removes panels from its tip up to the first ancestor that another branch still uses. The last branch cannot be closed (D5).
- **Activating a panel** keeps the active branch if it passes through the panel; otherwise it chooses the most recently active branch through it (D6).
- **Back and forward** are a jump list of panel activations (D2). Jumps never create panels and skip closed ones.
- **Focus Mode is a flag.** It never touches panels, branches or the active panel. A test proves the workspace is otherwise identical with it on or off.
- **IDs:** `PanelId` and `BranchId` are `u64` values from a workspace counter. They are stable, persisted and deterministic in tests.
- **Panel state** (`PanelState`) belongs to the panel: cursor, scroll anchor, view options (including the panel's own search), view mode and selection.
- **Restoring:** `Workspace::check_invariants` is used by the tests and by `from_parts` when a saved workspace is restored.

The view value types (`SortKey`, `SortOrder`, `ViewMode`, `ViewOptions`) moved to the domain so panel state can own them (D19). They are re-exported from `seshat_application::view`.

### Application: a functional core with effects

`seshat_application::workspace::WorkspaceService` owns the `Workspace` and a cache of per-panel listings. Every navigation, selection and setting change goes through it. Keys, mouse, menus and the command line all call the same methods (spatial §7.2, §9).

The service does no I/O. It returns `Effect`s (`Load`, `Watch`, `Unwatch`, `Launch`, `Persist`), and the presentation shell carries them out with the existing ports. This keeps the service deterministic. Tests answer `Load` effects from an in-memory tree and count reloads exactly, which is how "Focus Mode and branch switching never reload" (§16.4, §16.8) is verified.

Other parts of the service:

- **Listings** are cached for the 16 most recently used panels (D13). Only cached panels are watched. Shown panels and the active panel are never evicted, and an evicted panel keeps its logical state.
- **Cursor and selection:**
  - A panel's cursor stays on a valid item across reloads, moving to the nearest item when its item disappears.
  - Selections only contain items the panel currently shows, so an operation never acts on files hidden by a filter or no longer present.
  - `targets()` is the workspace-wide selection in spatial order, or else the active panel's cursor item.
- **Transfers:** `TransferPlan` turns the selection, grouped by panel, and an explicit destination into an ordinary `OperationRequest`. Paths are captured when the plan is made. The operation manager is unchanged and remains the single source of truth; D-Bus consumers see the same lifecycle as before.

`seshat_application::interaction` holds the keyboard model as pure logic:

- Vim key notation (`gg`, `<C-d>`).
- A `KeyMap` built from defaults plus user overrides, rejecting conflicting and prefix-overlapping bindings.
- The `Interaction` mode machine (Normal, Visual, Command). Focus Mode is not a mode.
- The command-line parser with completion and history.

### Infrastructure

- `TomlWorkspaceStore` persists the workspace to `$XDG_STATE_HOME/seshat/workspace.toml` (D16).
  - The format is versioned, with a migration hook; files from newer versions are ignored.
  - Writes are atomic.
  - Files are capped at 8 MiB.
  - Non-UTF-8 paths are stored as bytes.
- Keybindings and workspace preferences are new optional sections of `settings.toml`, so older files still load.

### Presentation: a shell around the service

- `App` keeps the window chrome. It renders the service after every message and carries out its effects.
- `WorkspaceView` renders the active branch's path as a horizontally scrolling strip.
  - Panels within three of the active panel get a full `PanelView`; farther ones are cheap collapsed buttons.
  - A branch bar lists every branch: switch, see the active branch, close.
  - A panel with several child branches shows a fork menu.
- `PanelView` holds no state. It renders a `PanelState` and listing and reports input, so dropping and recreating it loses nothing (§5.3).
- **Focus Mode** hides the other panel widgets without destroying them, so nothing reloads or is duplicated (§6, §12.4).
- **Keyboard:** a capture-phase key controller feeds GDK key events, converted to `KeyPress`, to the shared mode machine. Unbound keys, and every key while a text field or menu has focus, reach GTK unchanged, so all existing Ctrl and Alt shortcuts, arrows and dialogs keep working.
- **Cursor:** keyboard focus and the cursor are kept in step. A focused row becomes its panel's cursor, and the cursor is drawn on bound rows with a style class.
- **Styling:** every panel uses the `surface` token, the same one as the selected navigation item. The active panel is an `accent` outline, never a different fill. No new theme tokens were needed, so existing custom themes remain valid.

## Consequences

- Workspace logic is tested without GTK: 51 domain tests, including randomised sequences that check invariants after every step, plus application tests named after the spatial §16 criteria.
- The UI end-to-end test `scripts/ui/workspace.sh` walks the spatial spec's §19 definition of done in a real session.
- The D-Bus protocol, the operation manager and the native Wayland setup are unchanged.
- Deliberate limits, tracked in [STATUS.md](../implementation/STATUS.md):
  - Inactive branches were only discoverable (branch bar, fork menus) here; [ADR 0005](0005-branch-layout.md) later draws them side by side.
  - Folders renamed outside Seshat are not followed (D14).
  - Panel states for features that do not exist yet (column configuration, expanded metadata, preview) are not modelled (D12).
