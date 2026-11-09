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
 application    WorkspaceService ── listings (LRU 16) ── selection / targets
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
| File operations and their lifecycle | `OperationManager`, the single source of truth, also published on D-Bus |
| Rendered widgets | `WorkspaceView` and `PanelView`; disposable at any time |

### Rules specific to the workspace

- Navigation state never lives in widgets: views are rebuilt from the service.
- Focus Mode changes presentation only, and never creates, destroys or duplicates panels.
- Operations capture absolute source and destination paths when they are planned. Switching or closing branches, or toggling Focus Mode, does not affect a running operation.
- Branches, cursors and selections never resolve sources through "the active panel at execution time".
