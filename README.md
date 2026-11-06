# Seshat

A native Wayland file explorer for Linux, written in Rust with GTK4 and Relm4.

> **Status:** Phases 1–2 (foundation, filesystem integration) are complete. You can browse with back/forward history, breadcrumbs and a typed address bar, see live directory updates, browse the Trash, and use mounted devices with mount and eject. File operations, the D-Bus progress service, sorting, search and grid view come in later phases. See `spec.md` §17.

## Keyboard

| Keys | Action |
|---|---|
| Alt+Left / Alt+Right, mouse back/forward | Back / Forward |
| Alt+Up | Parent folder |
| F5, Ctrl+R | Refresh |
| Ctrl+H | Show hidden files |
| Ctrl+L | Type a location (`~/…`, `/…`, `trash:`), Esc to cancel |
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

## Layout

```
crates/
  seshat-domain/          entities, operation lifecycle, progress
  seshat-application/     ports and use cases
  seshat-infrastructure/  GIO-backed filesystem and XDG locations
  seshat-presentation/    GTK4/Relm4 UI, theme system, backend diagnostics
  seshat/                 binary (composition root)
```
