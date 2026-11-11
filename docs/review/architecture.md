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

## Design review follow-ups (2026-11-11)

- **One dispatcher (R2), done.** Keys, menus, the toolbar, the mouse back and forward buttons and the command line all carry out shared actions through `App::perform(Action)`. `Msg::Perform(Action)` replaced the separate `Back`, `Forward`, `GoUp`, `CycleBranch` and `ToggleFocusMode` messages, and `Command::as_action` sends `:branch next`, `:branch prev` and `:focus` to the same place. A new shared action is therefore handled in one place.
- **File action rules in the application layer (R1), done.** `seshat_application::workspace::file_actions` decides what rename, Trash, delete, restore, new item, empty Trash and the clipboard act on, returning an `ActionPlan` (submit, confirm or ask for a name) or a typed `ActionRefusal`. The window only draws the dialogs, through one `App::carry_out`. The menus read `Availability` from the same module, so menus and handlers can no longer disagree. `TransferPlan::from_paths` replaced the placeholder `PanelId` that clipboard and drag-and-drop transfers used.
- **Fuzzy Find matcher and walker behind ports (R4), done.** `FuzzyMatcher` and `TreeWalker` are application ports; `NucleoMatcher` and `LocalTreeWalker` implement them in infrastructure, and the architecture check forbids matcher crates elsewhere. Scopes are one function per scope, matched on a closed enum, which is the idiomatic Rust form of the strategy suggested in the review. See [search.md](../architecture/search.md).
