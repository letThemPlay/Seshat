# Linux and Wayland setup

## With Nix (recommended)

```sh
nix develop          # or: direnv allow (uses .envrc)
cargo run -p seshat
```

The dev shell provides everything: the Rust toolchain, GTK, Weston and Sway for the headless tests, `bubblewrap`, `wtype`, `grim` and `dbus-daemon`. It also sets `GDK_BACKEND=wayland`, so a missing Wayland session fails loudly instead of falling back to X11.

## Without Nix

You need Rust ≥ 1.93 (via rustup) and these development packages:

| Distribution | Packages |
|---|---|
| Fedora | `gtk4-devel glib2-devel pkgconf-pkg-config` |
| Debian/Ubuntu (24.10+) | `libgtk-4-dev libglib2.0-dev pkg-config` |
| Arch | `gtk4 glib2 pkgconf` |

GTK must be at least 4.20. Older distributions ship older GTK 4 versions and won't build. For the optional test tools, install `weston`, `sway`, `wtype`, `grim`, `bubblewrap` and `dbus`.

## Running under Wayland

- **Sessions:** use a Wayland session (GNOME, KDE Plasma, or a wlroots compositor such as Sway or Hyprland). Seshat doesn't need XWayland.
- **Check:** `seshat --check-backend` prints e.g. `display backend: wayland (native, wayland-0)`.
- **Logs:** every launch logs `display backend verified` at `info` level (`RUST_LOG=info`, the default).
- **Under XWayland by mistake,** the log shows `ERROR not running as a native Wayland client backend=x11 via XWayland`. Look for `GDK_BACKEND=x11` in your environment, desktop file overrides, or a missing `WAYLAND_DISPLAY`.

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| No "Trash" contents, no devices | GVfs not running | install or enable GVfs (`services.gvfs.enable = true` on NixOS) |
| Generic icons, empty "Type" column | no MIME database | install `shared-mime-info` (bundled in the Nix package) |
| Desktop theme overrides colours | (shouldn't happen) | Seshat's stylesheet has priority over `~/.config/gtk-4.0/gtk.css` by design ([ADR 0001](../decisions/0001-ui-framework.md)) |
| "D-Bus service unavailable" in the log | another Seshat instance owns the name, or no session bus | expected; the second instance runs without publishing |
