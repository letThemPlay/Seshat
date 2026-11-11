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
- **Size (updated 2026-11-11, R3):** `presentation/app/mod.rs` is down from about 1,060 to 881 lines, `Msg` from 62 to 47 variants and `App` from 37 to 26 fields. Themes, devices, operations and the command line are controllers (see [ARCHITECTURE.md](../architecture/ARCHITECTURE.md#presentation-structure)). What remains in `App` is mostly `init` (wiring) and `update_view` (rendering); the panel search bar and the address bar are candidates for the same treatment if they grow.

## Design review follow-ups (2026-11-11)

- **One dispatcher (R2), done.** Keys, menus, the toolbar, the mouse back and forward buttons and the command line all carry out shared actions through `App::perform(Action)`. `Msg::Perform(Action)` replaced the separate `Back`, `Forward`, `GoUp`, `CycleBranch` and `ToggleFocusMode` messages, and `Command::as_action` sends `:branch next`, `:branch prev` and `:focus` to the same place. A new shared action is therefore handled in one place.
- **File action rules in the application layer (R1), done.** `seshat_application::workspace::file_actions` decides what rename, Trash, delete, restore, new item, empty Trash and the clipboard act on, returning an `ActionPlan` (submit, confirm or ask for a name) or a typed `ActionRefusal`. The window only draws the dialogs, through one `App::carry_out`. The menus read `Availability` from the same module, so menus and handlers can no longer disagree. `TransferPlan::from_paths` replaced the placeholder `PanelId` that clipboard and drag-and-drop transfers used.
- **Fuzzy Find matcher and walker behind ports (R4), done.** `FuzzyMatcher` and `TreeWalker` are application ports; `NucleoMatcher` and `LocalTreeWalker` implement them in infrastructure, and the architecture check forbids matcher crates elsewhere. Scopes are one function per scope, matched on a closed enum, which is the idiomatic Rust form of the strategy suggested in the review. See [search.md](../architecture/search.md).
- **Remaining duplication, done (2026-11-11).** Navigation steps in `WorkspaceService` share `navigate`/`try_navigate`; `Opened` is built by `Opened::existing` and `Opened::created`; enums with stable text ids (`SortKey`, `ViewMode`, `WorkspaceLayout`, `Action`) are declared with `seshat_domain::stable_ids!`, so `ALL` can no longer miss a variant; branch numbering is `Workspace::branch_number`; the transfer confirmation heading moved to the window and uses `notice::count`. Left as they are, on purpose: `ThemeChoice` ids (it carries data, so it doesn't fit the macro), `OperationKind::id` (part of the D-Bus protocol, with no lookup), and the display words still in inner layers (`SortKey::label`, `OperationKind::verb`, `TransferKind::verb`), which are short, used by several layers, and would only move again if the UI is ever translated.
- **Listing cache and jump list extracted (R6), done.** `WorkspaceService` keeps its listings in a `ListingCache` (LRU, visible and active panels protected, generations), and `Workspace` keeps back/forward in a `JumpList` (a `VecDeque`, so the size limit no longer shifts the whole list; one `forget` instead of two copies). Both have their own tests, the saved format is unchanged, and the unused `NavigationHistory` was deleted.
- **Typed status messages (R5), done.** Messages are `Notice` values with one formatter and a severity (Info or Error); errors are styled with the error colour and announced to screen readers. About 45 places that built their own text now pass typed errors or call named constructors, and `capitalise`, `plural`, `left_out_notice` and `item_count` are gone. The Visual Mode refusal no longer assumes the refused action is pruning.
- **Feature controllers (R3), done.** `App` coordinates `ThemeController`, `DeviceController`, `OperationsController` and `CommandLineController`. Each one owns its own state and nested message enum and borrows shared state through `Cx`. Theme fallback notices and device removal are now unit-tested without GTK.

## Patterns and principles review (2026-11-11)

A second review against the specifications, design patterns and SOLID, DRY and YAGNI. It found the architecture sound: the layer rules hold, and the patterns in use (ports and adapters, effects, observer, strategy, mediator, state machine, memento) fit. Its recommendations are delivered as small, separate pull requests in this order. Numbered P1 to P10 to keep them apart from R1 to R6 above.

| # | Recommendation | Status |
|---|---|---|
| P2 | All status-bar wording in `notice.rs`, enforced by the compiler | **Done**: about 20 call sites that built their own text now call named constructors (`notice::no_branch`, `notice::not_pruned`, …). `Notice::info` and `Notice::error` are private to the module, so wording can't be added elsewhere. The duplicated "There is no branch N" is one function. Wording and severity are unchanged and pinned by `named_messages_keep_their_wording_and_severity`. |
| P3 | Enums instead of boolean flags (`cut`, `directory`, `strip`, `forward`), reusing `TransferKind`, `NameKind` and `WorkspaceLayout` | **Done**: the clipboard, the staged-files line and `Msg::ClipboardStore` carry a `TransferKind` (copy or move), so pasting no longer converts a `cut` flag back into one. New items take `file_actions::ItemKind` (folder or file). `WorkspaceView` takes the `WorkspaceLayout` preference. Branch cycling and command-line completion take `seshat_domain::Direction`, whose `wrapping_step` replaces the two copies of the wrap-around arithmetic. Left as they are: `FileEntry`'s `to_directory` (data, not a flag), the crate-private `simple::create`, and `JumpList::step` (back and forward history, not cycling). No change in behaviour. |
| P9 | Parse ids at the boundary; one address-resolving helper; preference mapping in the application layer | **Done**: `Msg::SetViewMode`, `Msg::SetSortKey` and `ThemeMsg::Set` carry `ViewMode`, `SortKey` and `ThemeChoice`, parsed where the window action arrives (`actions.rs`); an unknown id is logged there instead of being dropped silently further in. The Ctrl+1 and Ctrl+2 shortcuts use `ViewMode` instead of the strings `"list"` and `"grid"`. The address bar, `:open` and `:copy`/`:move` share `App::resolve_address`. `Preferences::panel_defaults` and `Preferences::adopt_panel_defaults` hold the mapping between saved preferences and new-panel settings (decision D11), which the window used to build field by field. No change in behaviour. |
| P1 | One dispatcher for file actions, view settings and refresh: `Action` ids replace the string tables in `actions.rs` and `shortcuts.rs`, and the command line's duplicate handlers | Planned |
| P5, P6 | Split `WorkspaceService` and the domain `Workspace` into files by concern, behind the same API; `PanelListing` owns its failure state | Planned |
| P7 | One `FileSystem` conformance suite run against both `MemoryFs` and `LocalFileSystem` | Planned |
| P4 | An effect runner that owns watches, reload debouncing and save scheduling, out of `App` | Planned |
| P10 | Address bar and panel search as controllers, before the Fuzzy Find overlay | Planned |
| P8 | Narrower `Cx` per controller | Planned |

Considered and not recommended: trait objects for file operations (the closed `OperationRequest` enum is serialisable and exhaustively checked), an observer on `Workspace` (the update-then-render loop already gives one-way data flow), more strategy types for search scopes, and a general undo stack before a second undoable change exists.
