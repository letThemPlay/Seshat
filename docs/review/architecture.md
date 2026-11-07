# Architecture review (Phase 6, 2026-11-07)

## Enforced automatically

`scripts/check-architecture.sh` (part of `scripts/check.sh`) checks the direct dependencies of each crate:

| Crate | Must not depend on |
|---|---|
| `seshat-domain` | UI toolkits, async runtimes, D-Bus, serialisation, OS bindings, any other Seshat crate |
| `seshat-application` | UI toolkits, async runtimes, D-Bus, infrastructure, presentation |
| `seshat-infrastructure` | GTK, Relm4, presentation |
| `seshat-presentation` | infrastructure, async runtimes and D-Bus, OS bindings; also no `std::fs` in its source |
| `seshat-progress` | any Seshat crate (protocol-only client) |

The check was verified to fail on a deliberate violation (domain → `glib`).

## Findings

- **Fixed: relm4's `gnome_49` feature silently pulled in libadwaita**, contradicting ADR 0001 and adding a runtime dependency. It's removed; the GTK API level comes from `gtk4`'s `v4_20` feature.
- **The single source of truth holds.** The UI and D-Bus both observe `OperationManager` events; neither keeps its own operation state.
- **Ports:** all I/O goes through ports (`DirectoryReader`, `DirectoryWatcher`, `DeviceMonitor`, `Launcher`, `ThemeRepository`, `PreferencesStore`, `FileSystem`). The composition root (`crates/seshat/src/main.rs`) is the only place concrete types meet.
- **The manager can move into its own process.** It's reachable only through application types (requests, events, snapshots), and the D-Bus adapter already speaks the full contract. A standalone daemon would reuse `IpcService` unchanged and add a proxy-backed client in the UI.
- **Size:** the largest file is `presentation/app/mod.rs` (~1,000 lines). It's split by concern already (`view`, `file_ops`, `shortcuts`); a further split (navigation and theme handling) is a reasonable follow-up.
