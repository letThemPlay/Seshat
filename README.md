# Seshat

A native Wayland file explorer for Linux, written in Rust with GTK4 and Relm4.

> **Status:** 0.1.0, all six phases of `spec.md` §17 are complete. See [Definition of Done](docs/definition-of-done.md) for what is verified and what still needs manual checks (GNOME and KDE sessions, removable devices).

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

## Install

```sh
nix run .                       # try it
nix profile install .#seshat    # install for your user
```

See [packaging](docs/packaging.md) for contents, dependencies and Wayland requirements. [Setup](docs/development/setup.md) covers building without Nix.

## Build and run

The toolchain comes from the Nix flake (Rust, GTK 4.20+, Weston for tests):

```sh
nix develop          # or: direnv allow
cargo run -p seshat
```

Other distributions need Rust ≥ 1.93, `pkg-config`, GTK ≥ 4.20 and GLib ≥ 2.86 development packages, and GVfs for Trash and network locations.

## Checks

```sh
scripts/check.sh --ui                # everything below, plus the UI end-to-end test
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
- [Wayland verification](docs/development/wayland-verification.md)
- [Writing themes](docs/development/themes.md)
- [Testing](docs/development/testing.md)
- [Linux and Wayland setup](docs/development/setup.md)
- [Packaging](docs/packaging.md)
- [Definition of Done](docs/definition-of-done.md)
- Reviews: [security](docs/review/security.md), [performance](docs/review/performance.md), [architecture](docs/review/architecture.md)
- ADRs: [0001 UI framework](docs/decisions/0001-ui-framework.md), [0002 IPC](docs/decisions/0002-ipc.md), [0003 packaging](docs/decisions/0003-packaging.md)
- [Changelog](CHANGELOG.md)

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
