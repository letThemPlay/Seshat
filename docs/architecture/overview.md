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
| `seshat-domain` | `FileEntry`, `Location`, `StandardLocation`, `OperationId`, `OperationStatus` (lifecycle with explicit transitions), `Progress` (determinate/indeterminate), `DomainError` | `thiserror`, `uuid` |
| `seshat-application` | Ports: `DirectoryReader`, `StandardLocations`. Use cases: `ListDirectory` (filtering, sorting) | domain |
| `seshat-infrastructure` | `GioDirectoryReader`, `XdgStandardLocations` | application, domain, gio/glib |
| `seshat-presentation` | Window, navigation pane, content pane, theme system, display-backend diagnostics | application, domain, gtk4, relm4 |
| `seshat` | `main`, CLI flags, logging setup, dependency wiring | everything |

The four layers are separate crates. Anything smaller (views, theme, diagnostics) is a module, to avoid unnecessary crate boundaries.

## Rules

- Presentation never touches the filesystem. It calls `ListDirectory`, which calls the `DirectoryReader` port.
- Ports return `LocalBoxFuture` (not `Send`) because GIO I/O runs on the GLib main context of the UI thread. CPU-heavy or blocking work (file copies, Phase 4) will run on a bounded worker pool and return results through channels.
- Names and paths are `OsString`/`PathBuf` end to end. `FileEntry::display_name` is lossy and display-only.
- Symlinks are never followed during listing (`NOFOLLOW_SYMLINKS`).
- Logs never include file listings or contents. Relm4 logs input messages at debug level, so `Msg::Loaded` prints only an entry count.

## File Operation Manager (Phases 4–5)

The domain already defines the operation identity, the lifecycle (`OperationStatus::transition` rejects invalid transitions) and progress. The manager itself will live in `seshat-application` as a service that owns all operation state. Presentation and the D-Bus adapter (infrastructure) will both be *observers* of its event stream. The manager is reached only through application-defined traits, so it can later move into a separate background process (a D-Bus-activated service) without changing the domain or application layers.

**Initial limitation (by design, to be documented again in Phase 4):** the manager runs inside the UI process. Closing the window will hide it rather than quit while operations are active. Killing the process will still abort them.

## Async model

```text
UI event ─▶ App::update ─▶ navigate(): generation += 1, state = Loading
                              └─ spawn_local(ListDirectory::execute) ─┐
                                                                      ▼ (GLib main loop, non-blocking)
App::update(Msg::Loaded{generation}) ◀─ result ───────────────────────┘
  └─ stale generation? drop : render entries / empty / error
```
