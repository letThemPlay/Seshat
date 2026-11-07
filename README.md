# Seshat

A native Wayland file explorer for Linux, written in Rust with GTK4 and Relm4.

> **Status:** Phases 1–5 are complete: foundation, filesystem integration, UI and theming, file operations, and D-Bus IPC. File operations run through a central Operation Manager with progress, cancellation, conflict resolution and partial-failure reporting, and they keep running if the window is closed. Other applications can follow and cancel them over D-Bus ([protocol](docs/ipc/protocol.md), example client `cargo run -p seshat-progress`). Phase 6 covers quality, packaging and release. See `spec.md` §17.

## Keyboard

| Keys | Action |
|---|---|
| Alt+Left / Alt+Right, mouse back/forward | Back / Forward |
| Alt+Up | Parent folder |
| F5, Ctrl+R | Refresh |
| Ctrl+H | Show hidden files |
| Ctrl+L | Type a location (`~/…`, `/…`, `trash:`), Esc to cancel |
| Ctrl+F, or start typing | Search this folder (Return or Down: go to the results, Esc: close) |
| Ctrl+1 / Ctrl+2 | List / grid view |
| Ctrl+A | Select all |
| Ctrl+X / Ctrl+C / Ctrl+V | Cut / copy / paste files |
| Ctrl+Shift+C | Copy selected paths |
| F2 | Rename |
| Delete / Shift+Delete | Move to Trash / delete permanently (confirmed) |
| Ctrl+Shift+N | New folder |
| Shift+F10, Menu | Context menu |
| Enter | Open folder or file |

## Build and run

The toolchain comes from the Nix flake (Rust, GTK 4.20+, Weston for tests):

```sh
nix develop          # or: direnv allow
cargo run -p seshat
```

Other distributions need Rust ≥ 1.93, `pkg-config`, GTK ≥ 4.20 and GLib ≥ 2.86 development packages, and GVfs for Trash and network locations.

## Checks

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
scripts/verify-wayland.sh           # headless Weston: proves native Wayland, no XWayland
scripts/ui-session.sh scripts/ui/file-operations.sh   # sandboxed UI end-to-end test
cargo run -p seshat -- --check-backend
```

## Documentation

- [Architecture overview](docs/architecture/overview.md)
- [Filesystem integration](docs/architecture/filesystem.md)
- [File operations](docs/architecture/operations.md)
- [D-Bus protocol](docs/ipc/protocol.md)
- [ADR 0001: UI framework](docs/decisions/0001-ui-framework.md)
- [Wayland verification](docs/development/wayland-verification.md)
- [Writing themes](docs/development/themes.md)
- [Testing](docs/development/testing.md)

## Layout

```
crates/
  seshat-domain/          entities, operation lifecycle, progress
  seshat-application/     ports and use cases
  seshat-infrastructure/  GIO-backed filesystem and XDG locations
  seshat-presentation/    GTK4/Relm4 UI, theme system, backend diagnostics
  seshat/                 binary (composition root)
  seshat-progress/        example D-Bus client (independent of the other crates)
```
