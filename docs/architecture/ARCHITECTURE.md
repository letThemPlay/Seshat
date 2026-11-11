# Architecture

Seshat follows Clean Architecture. Crate boundaries enforce it, and `scripts/check-architecture.sh` checks them.

- **Layers and crates:** [overview.md](overview.md)
- **Filesystem integration:** [filesystem.md](filesystem.md)
- **File operations:** [operations.md](operations.md)
- **D-Bus protocol:** [../ipc/protocol.md](../ipc/protocol.md)
- **Decisions:** [DECISIONS.md](DECISIONS.md)
- **Linux and Wayland:** [LINUX-WAYLAND.md](LINUX-WAYLAND.md)
- **Fuzzy Find search engine:** [search.md](search.md)

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
| Fuzzy Find sessions, their search generations and cached candidates | `SearchSessions` and `SearchEngine` (application), for one session; never saved ([search.md](search.md)) |
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

## Presentation structure

The window (`seshat-presentation/src/app/`) is a Relm4 component, `App`, that coordinates feature controllers (the Mediator pattern). Each controller is a plain struct that `App` owns:

| Controller | Owns | Messages |
|---|---|---|
| `ThemeController` (`app/theme.rs`) | The chosen theme, custom theme registry, themes-folder watch and reload, stylesheet | `Msg::Theme(ThemeMsg)` |
| `DeviceController` (`app/devices.rs`) | The device list, mounting and ejecting | `Msg::Device(DeviceMsg)` |
| `OperationsController` (`app/operations.rs`) | The operations panel, conflict dialogs, staying alive while hidden; observes the `OperationManager` | `Msg::Operations(OperationsMsg)` |
| `CommandLineController` (`app/command_line.rs`) | The command line's field, history and Tab completion | `Msg::CommandLine(CommandLineMsg)` |

Rules:

- A window feature with its own state gets a controller and a nested message enum, not new fields and variants on `App`. Fuzzy Find's overlay will be the next one.
- Controllers borrow what they share (the `WorkspaceService`, the status notice, the sender) through `Cx`, for one call at a time. They never keep a copy of workspace or operation state.
- A controller never reaches into `App` or another controller. When something else must happen (saving preferences, updating the sidebar, closing the command line, running a command), it says so in its return value and `App` does it. Ordering that spans controllers stays visible in `App` (for example, releasing watches on a device before ejecting it).
- Status-bar messages are `Notice` values (`seshat-presentation/src/notice.rs`). Only that module formats message text: call sites pass typed errors or call a named constructor. Every message is a sentence, and each is Info or Error: errors are drawn in the theme's error colour and announced to screen readers (decisions N1–N3).
- Navigation, workspace rendering, keyboard intents, file actions and running commands stay in `App` (`commands.rs`, `file_ops.rs`, `workspace.rs`), because they coordinate every part of the window.
