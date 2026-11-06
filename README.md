# Seshat

A native Wayland file explorer for Linux, written in Rust with GTK4 and Relm4.

> **Status:** Phases 1–3 are complete: foundation, filesystem integration, and the UI and theming. You can browse with history, breadcrumbs and an address bar; use list and grid views, natural sorting, search, multi-selection, context menus and drag-out; see live updates; browse the Trash and devices; and use light, dark and custom themes. Preferences are saved between sessions. File operations (copy, move, rename, delete) and the D-Bus progress service come in Phases 4–5. See `spec.md` §17.

## Keyboard

| Keys | Action |
|---|---|
| Alt+Left / Alt+Right, mouse back/forward | Back / Forward |
| Alt+Up | Parent folder |
| F5, Ctrl+R | Refresh |
| Ctrl+H | Show hidden files |
| Ctrl+L | Type a location (`~/…`, `/…`, `trash:`), Esc to cancel |
| Ctrl+F, or start typing | Search this folder |
| Ctrl+1 / Ctrl+2 | List / grid view |
| Ctrl+A | Select all |
| Ctrl+Shift+C | Copy selected paths |
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
cargo run -p seshat -- --check-backend
```

## Documentation

- [Architecture overview](docs/architecture/overview.md)
- [Filesystem integration](docs/architecture/filesystem.md)
- [ADR 0001: UI framework](docs/decisions/0001-ui-framework.md)
- [Wayland verification](docs/development/wayland-verification.md)
- [Writing themes](docs/development/themes.md)

## Layout

```
crates/
  seshat-domain/          entities, operation lifecycle, progress
  seshat-application/     ports and use cases
  seshat-infrastructure/  GIO-backed filesystem and XDG locations
  seshat-presentation/    GTK4/Relm4 UI, theme system, backend diagnostics
  seshat/                 binary (composition root)
```
