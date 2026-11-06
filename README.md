# Seshat

A native Wayland file explorer for Linux, written in Rust with GTK4 and Relm4.

> **Status:** Phase 1 (research and foundation) is complete. The app browses directories and switches themes, and it has been verified to run as a native Wayland client. File operations, the D-Bus progress service, Trash and devices come in later phases. See `spec.md` §17.

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
