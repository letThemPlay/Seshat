# Changelog

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
