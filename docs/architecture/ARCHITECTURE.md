# Architecture

Seshat follows Clean Architecture. Crate boundaries enforce it, and `scripts/check-architecture.sh` checks them.

- **Layers and crates:** [overview.md](overview.md)
- **Filesystem integration:** [filesystem.md](filesystem.md)
- **File operations:** [operations.md](operations.md)
- **D-Bus protocol:** [../ipc/protocol.md](../ipc/protocol.md)
- **Decisions:** [DECISIONS.md](DECISIONS.md)
- **Linux and Wayland:** [LINUX-WAYLAND.md](LINUX-WAYLAND.md)

## The spatial workspace

[ADR 0004](../decisions/0004-spatial-workspace.md) has the full rationale.

```text
 keys ─┐  mouse ─┐  menus ─┐  ":" command line ─┐
       ▼         ▼         ▼                     ▼
 presentation   App (shell): KeyRouter (mode machine) → WorkspaceService calls
                │   renders WorkspaceView ─ PanelView (stateless) ─ FileView
                │   carries out Effects: Load / Watch / Unwatch / Launch / Persist
                ▼
 application    WorkspaceService ── listings (LRU 16) ── selection / targets ── undo (prunes, session only)
                │   TransferPlan ──► OperationRequest ──► OperationManager (unchanged) ──► D-Bus (unchanged)
                │   interaction: KeyMap, Interaction (Normal/Visual/Command), command parser
                ▼
 domain         Workspace: forest of Panels (PanelState) + Branches (tip, cursor) + jump list + Focus Mode flag
 infrastructure TomlWorkspaceStore ($XDG_STATE_HOME/seshat/workspace.toml), settings sections, GIO ports
```

### Where state lives

| State | Owner |
|---|---|
| Panels, branches, active branch, jump list, Focus Mode | `seshat_domain::workspace::Workspace` |
| Per-panel cursor, scroll anchor, sort, view mode, hidden files, search, selection | `PanelState`, inside the workspace |
| Directory listings and their load and watch generations | `WorkspaceService` (cached per panel; never in widgets) |
| Interaction mode, pending keys | `Interaction` (shared with the key controller) |
| Workspace undo history (pruned panels with their state) | `WorkspaceService`, for the session; never saved ([ADR 0007](../decisions/0007-backward-branch-pruning.md)) |
| File operations and their lifecycle | `OperationManager`, the single source of truth, also published on D-Bus |
| Rendered widgets | `WorkspaceView` (a grid laid out by the pure `workspace_layout`: depth across, branches down, connectors), `PanelView` and `BranchCard`; disposable at any time |

### Rules specific to the workspace

- Navigation state never lives in widgets: views are rebuilt from the service.
- One dispatcher: an action that a key, menu, toolbar button, mouse button or command can trigger is an `Action` and runs through `App::perform` (`Msg::Perform`). A command maps to its action with `Command::as_action`. Never add a second handler for the same action.
- Focus Mode changes presentation only, and never creates, destroys or duplicates panels.
- File action rules live in the application layer (`workspace::file_actions`): what rename, Trash, delete, restore, new item, empty Trash and the clipboard act on, and when they are refused. The window shows the confirmation or name prompt an `ActionPlan` asks for and submits its captured request; it never decides targets itself.
- Operations capture absolute source and destination paths when they are planned. Switching or closing branches, or toggling Focus Mode, does not affect a running operation.
- Branches, cursors and selections never resolve sources through "the active panel at execution time".
- Pruning (`db`, `dB`) and workspace undo change panels and branches only. They go through `WorkspaceService`, whose only effects are unwatching and saving, and they never reach the operation manager or the filesystem. Pruning removes only panels no other branch uses ([ADR 0007](../decisions/0007-backward-branch-pruning.md)).
