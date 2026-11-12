# Architecture overview

Seshat follows Clean Architecture. Dependencies point inwards and are enforced by crate boundaries: a crate can only use what its `Cargo.toml` lists.

```text
             ┌──────────────────────┐
             │ seshat (bin)         │  composition root: wires impls → use cases → UI
             └──┬────────┬───────┬──┘
                │        │       │
   ┌────────────▼──┐  ┌──▼───────────────┐
   │ presentation  │  │ infrastructure   │   GTK4/Relm4  |  GIO, (later) zbus, config
   └──────┬────────┘  └──┬───────────────┘
          │              │ implements ports
        ┌─▼──────────────▼─┐
        │ application      │   use cases, ports (traits)
        └────────┬─────────┘
               ┌─▼──────┐
               │ domain │       entities, value objects, invariants — no deps on
               └────────┘       UI, D-Bus, tokio or concrete filesystems
```

## Crates

| Crate | Owns | May depend on |
|---|---|---|
| `seshat-domain` | `FileEntry`, `Location` (path or Trash-relative), `Device`, `StandardLocation`, `OperationId`, `OperationStatus` (lifecycle with explicit transitions), `Progress` (determinate/indeterminate), `DomainError`, `Direction` (wrap-around stepping through a list); view value types (`SortOrder`, `ViewMode`, `ViewOptions`); the spatial **`Workspace`** (panels, branches, panel state, invariants) and its `JumpList`; Fuzzy Find value types (`search`) | `thiserror`, `uuid` |
| `seshat-application` | Ports: `DirectoryReader`, `DirectoryWatcher`, `DeviceMonitor`, `StandardLocations`, `Launcher`, `TreeWalker`, `FuzzyMatcher`. Fuzzy Find: `search` (scopes, ranking, `SearchEngine`, `SearchSessions`, `SearchWorker`). Use cases: `ListDirectory`. Navigation: `breadcrumbs`, `parse_address` (back/forward is the workspace's `JumpList`). View: `arrange` (filter, search, natural sort). `Preferences` + `PreferencesStore` port, `ThemeRepository` port. **`WorkspaceService`** (navigation, selection, backward pruning and workspace undo, effects; listings in its `ListingCache`), `file_actions` (what rename, Trash, delete, restore, new item, empty Trash and the clipboard act on, and whether they may run), `TransferPlan`, `WorkspaceStore` port; `interaction` (window actions `AppAction`, key notation, keymap, modes, command line) | domain |
| `seshat-infrastructure` | GIO implementations of the filesystem ports (see [filesystem.md](filesystem.md)); `LocalFileSystem` for operations; TOML settings, theme files and the saved workspace (`persistence`); the D-Bus service (`ipc`, see [protocol](../ipc/protocol.md)); Fuzzy Find adapters (`search`: `LocalTreeWalker`, `NucleoMatcher`) | application, domain, gio/glib, nucleo-matcher |
| `seshat-presentation` | Window (`app/`, a shell around the workspace service that coordinates the theme, device, operations and command-line controllers), workspace strip and panels (`workspace_view`, `panel_view`), key routing (`keys`), sidebar component, path bar, list/grid file views (`files`), actions and menus, shortcuts, theme system, display-backend diagnostics | application, domain, gtk4, relm4 |
| `seshat` | `main`, CLI flags, logging setup, dependency wiring, starts the D-Bus service | everything |
| `seshat-progress` | Example D-Bus client; speaks only the protocol | zbus |

The four layers are separate crates. Anything smaller (views, theme, diagnostics) is a module, to avoid unnecessary crate boundaries.

## Rules

- Presentation never touches the filesystem. It calls `ListDirectory`, which calls the `DirectoryReader` port.
- Ports return `LocalBoxFuture` (not `Send`) because GIO I/O runs on the GLib main context of the UI thread. CPU-heavy or blocking work (file copies, Phase 4) will run on a bounded worker pool and return results through channels.
- Names and paths are `OsString`/`PathBuf` end to end. `FileEntry::display_name` is lossy and display-only.
- Symlinks are never followed during listing (`NOFOLLOW_SYMLINKS`).
- Logs never include file listings or contents. Relm4 logs input messages at debug level, so `Msg::Loaded` prints only an entry count.

## Spatial workspace

See [ARCHITECTURE.md](ARCHITECTURE.md) and [ADR 0004](../decisions/0004-spatial-workspace.md): panel and branch state lives in the domain `Workspace`, the `WorkspaceService` turns input into state changes and I/O effects, and the UI is rebuilt from it.

## File Operation Manager

Implemented in `seshat-application::operations`; see [operations.md](operations.md) for the lifecycle, event and snapshot contract, safety rules and how to add an operation.

## Async model

```text
UI event ─▶ App::update ─▶ navigate(): generation += 1, state = Loading
                              └─ spawn_local(ListDirectory::execute) ─┐
                                                                      ▼ (GLib main loop, non-blocking)
App::update(Msg::Loaded{generation}) ◀─ result ───────────────────────┘
  └─ stale generation? drop : render entries / empty / error
```
