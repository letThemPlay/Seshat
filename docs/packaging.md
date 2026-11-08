# Packaging

Seshat currently ships as a **Nix flake package** (see [ADR 0003](decisions/0003-packaging.md)).

## Using the package

```sh
nix run github:<owner>/seshat              # or, in a checkout: nix run .
nix build .#seshat && ./result/bin/seshat
nix profile install .#seshat               # per-user install
nix run .#seshat-progress                  # the example D-Bus client
```

On NixOS, add the flake as an input and put `inputs.seshat.packages.${system}.default` in `environment.systemPackages` (or `home.packages` with Home Manager).

## What the package contains

| Path | Purpose |
|---|---|
| `bin/seshat` | the application (GTK wrapper around `.seshat-wrapped`) |
| `bin/seshat-progress` | example D-Bus client |
| `share/applications/dev.kdmorris.Seshat.desktop` | desktop entry (validated with `desktop-file-validate`) |
| `share/icons/hicolor/scalable/apps/dev.kdmorris.Seshat.svg` | application icon |
| `share/metainfo/dev.kdmorris.Seshat.metainfo.xml` | AppStream metadata (`appstreamcli validate`: one warning, the missing homepage URL) |

The build runs the whole workspace test suite in the Nix sandbox, including the D-Bus tests on a private bus.

## Dependencies

**Build:** Rust ≥ 1.93, `pkg-config`, GTK ≥ 4.20 and GLib ≥ 2.86 development files.

**Runtime:**
- GTK 4 and GLib/GIO
- `shared-mime-info`, for file types and icons; the Nix package bundles it
- GVfs (optional), for the Trash location, network mounts and the device list
- a desktop portal (optional), for opening files from a sandbox
- a Wayland compositor

## Wayland requirements

- **Seshat needs a Wayland compositor and does not need XWayland.**
- Nothing in the package sets `GDK_BACKEND`. The GTK wrapper only sets data directories, GIO modules and the pixbuf loader cache, so GTK picks the Wayland backend whenever `WAYLAND_DISPLAY` is set.
- Every launch logs the backend GTK actually connected to ("display backend verified").
- `seshat --check-backend` exits non-zero unless the backend is native Wayland.
- `SESHAT_BIN=./result/bin/seshat scripts/verify-wayland.sh` verifies the packaged binary in a headless compositor.

## D-Bus

- Seshat owns `dev.kdmorris.Seshat.Operations` on the **session bus** while it runs ([protocol](ipc/protocol.md)). GApplication separately owns the application id `dev.kdmorris.Seshat`.
- There is no D-Bus activation file. The service has nothing to report unless Seshat is running, and activating it would open a window. A future standalone operations daemon would add a `.service` file.
- No system-bus access and no polkit actions are needed.

## Permissions

- **Files:** the user's normal filesystem access. There is no setuid helper.
- **Session bus:** owns one name, and talks to GVfs and portals through GIO.
- **No X11, no network access** (apart from GVfs network mounts the user opens) and no system bus.

## Other formats (not yet built)

Notes for when these are added:

- **Flatpak:**
  - `--socket=wayland` and **no** `--socket=fallback-x11`; otherwise the app could silently run under XWayland.
  - `--filesystem=host` (a file manager needs it).
  - `--own-name=dev.kdmorris.Seshat.Operations` and `--talk-name=org.gtk.vfs.*`.
  - The Trash and device features depend on host GVfs access.
- **AppImage:** GTK4 AppImages bundle their own GTK and often break theming and portals. That's lower priority.
- **DEB/RPM:** these are straightforward from `cargo build --release` plus the `data/` files, using the dependencies listed above.
