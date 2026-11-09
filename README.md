# Seshat

[![CI](https://github.com/letThemPlay/Seshat/actions/workflows/ci.yml/badge.svg)](https://github.com/letThemPlay/Seshat/actions/workflows/ci.yml)
[![Security](https://github.com/letThemPlay/Seshat/actions/workflows/security.yml/badge.svg)](https://github.com/letThemPlay/Seshat/actions/workflows/security.yml)

A native Wayland file explorer for Linux, written in Rust with GTK4 and Relm4: a spatial, keyboard-first workspace of persistent folder panels and branches.

> **Status:** 0.1.0. All six phases of the [original specification](docs/specifications/0-INITIAL-PRODUCT-SPEC.md) and of the [spatial file manager](docs/specifications/1-SPATIAL-FILE-MANAGER.md) are implemented. See [implementation status](docs/implementation/STATUS.md) and the [Definition of Done](docs/definition-of-done.md) for what is verified and what still needs manual checks.

## The workspace

Opening a folder adds a **panel** to the right instead of replacing the view. Panels keep their cursor, scroll position, sort order, view, search and selection while you move around. Opening a different folder from a panel that already has a child starts a new **branch**. Branches coexist side by side: each row is a branch, a shared folder spans the rows of its branches, and connectors link folders to their subfolders. The active branch gets the space; the others are compact cards you can click. **Focus Mode** (`zz`) expands the active panel without closing anything; navigation keeps working. The workspace is restored when Seshat starts.

Keys follow Vim. `:help` lists every key and command. Bindings can be changed in `~/.config/seshat/settings.toml`:

```toml
[keybindings]
focus-mode = ["zz", "<C-f>"]

[workspace]
classic_keys = false   # true: typing searches, no single-letter keys
wrap_cursor = false
layout = "tree"         # "strip": only the active branch, as one row
retain_focus_mode = true
restore = true
```

| Keys | Action |
|---|---|
| `h` / `l` | Previous / next panel of the branch |
| `j` / `k`, `gg` / `G`, `Ctrl+d` / `Ctrl+u` | Move the cursor; first / last item; half a page |
| `Enter` / `Backspace` | Open the folder (new panel) or file / parent panel |
| `[` / `]` | Previous / next branch |
| `v` / `V` / `Esc` | Visual Mode: toggle item / range / leave (selection kept); `Esc` again clears the selection |
| `y` / `x` / `p` | Copy / cut the selection (from any panels) / paste into this panel |
| `zz` | Focus Mode |
| `/` | Search this panel |
| `:` | Command line: `:copy`, `:move [path \| bN]`, `:delete`, `:rename`, `:mkdir`, `:touch`, `:sort`, `:view`, `:hidden`, `:branch [N \| next \| prev \| close]`, `:focus`, `:open`, `:help`. Tab completes commands, folders and `bN`; Tab / Shift+Tab again cycles through matches |
| Alt+Left / Alt+Right, Ctrl+O / Ctrl+I, mouse back/forward | Back / Forward (previously active panels) |
| Alt+Up | Parent panel |
| F5, Ctrl+R | Refresh |
| Ctrl+H | Show hidden files |
| Ctrl+L | Type a location (`~/…`, `/…`, `trash:`), Esc to cancel |
| Ctrl+F, `/` | Search this panel (Return or Down: go to the results, Esc: close) |
| Ctrl+1 / Ctrl+2 | List / grid view |
| Ctrl+A | Select all |
| Ctrl+X / Ctrl+C / Ctrl+V | Cut / copy / paste files |
| Ctrl+Shift+C | Copy selected paths |
| F2 | Rename |
| Delete / Shift+Delete | Move to Trash / delete permanently (confirmed) |
| Ctrl+Shift+N | New folder |
| Shift+F10, Menu | Context menu |

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
scripts/ui-session.sh scripts/ui/workspace.sh         # the spatial workspace, end to end
cargo run -p seshat -- --check-backend
```

## Documentation

- Specifications: [original](docs/specifications/0-INITIAL-PRODUCT-SPEC.md), [spatial file manager](docs/specifications/1-SPATIAL-FILE-MANAGER.md)
- [Implementation status](docs/implementation/STATUS.md) and [roadmap](docs/implementation/ROADMAP.md)
- [Architecture](docs/architecture/ARCHITECTURE.md), [decisions](docs/architecture/DECISIONS.md), [Linux and Wayland](docs/architecture/LINUX-WAYLAND.md)
- [Architecture overview](docs/architecture/overview.md)
- [Filesystem integration](docs/architecture/filesystem.md)
- [File operations](docs/architecture/operations.md)
- [D-Bus protocol](docs/ipc/protocol.md)
- [Wayland verification](docs/development/wayland-verification.md)
- [Writing themes](docs/development/themes.md)
- [Testing](docs/development/testing.md)
- [Continuous integration](docs/development/ci.md)
- [Linux and Wayland setup](docs/development/setup.md)
- [Packaging](docs/packaging.md)
- [Definition of Done](docs/definition-of-done.md)
- Reviews: [security](docs/review/security.md), [performance](docs/review/performance.md), [architecture](docs/review/architecture.md)
- ADRs: [0001 UI framework](docs/decisions/0001-ui-framework.md), [0002 IPC](docs/decisions/0002-ipc.md), [0003 packaging](docs/decisions/0003-packaging.md), [0004 spatial workspace](docs/decisions/0004-spatial-workspace.md), [0005 branch layout](docs/decisions/0005-branch-layout.md)
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

## Licence

Copyright (C) 2026 the Seshat contributors.

Seshat is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. It is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See [LICENSE](LICENSE) for the full text.
