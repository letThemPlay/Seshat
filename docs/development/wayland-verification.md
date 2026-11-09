# Wayland verification

Native Wayland operation is a release requirement. Running under XWayland fails it.

## How the backend is detected

`seshat_presentation::diagnostics::detect_backend` inspects the concrete type of the default `GdkDisplay`:

- `GdkWaylandDisplay` → native Wayland (pass)
- `GdkX11Display` → X11. Inside a Wayland session this means XWayland (fail)
- anything else, or no display → fail

`WAYLAND_DISPLAY`, `XDG_SESSION_TYPE`, `DISPLAY` and `GDK_BACKEND` are logged next to the result for diagnosis. They are never used as proof.

Every launch logs one of:

```
INFO  display backend verified backend=wayland (native, wayland-1) ...
ERROR not running as a native Wayland client backend=x11 via XWayland (:0) ...
```

## Automated check

```sh
nix develop
scripts/verify-wayland.sh
```

The script starts an isolated headless **Weston** compositor in a temporary `XDG_RUNTIME_DIR`, with no X server and `DISPLAY` unset. It then checks that:

1. `seshat --check-backend` reports `wayland (native, <socket>)` and exits 0;
2. the full UI, run with `WAYLAND_DEBUG=1`, creates an `xdg_surface`/`xdg_toplevel` and logs "display backend verified";
3. `--check-backend` exits non-zero when no compositor is reachable.

The dev shell sets `GDK_BACKEND=wayland`, so GTK fails instead of silently falling back to X11.

## Quick manual checks

```sh
cargo run -p seshat -- --check-backend              # expect exit 0
GDK_BACKEND=x11 cargo run -p seshat -- --check-backend   # expect exit 1 (negative control)
```

Compositor-specific checks:

- Hyprland: `hyprctl clients` should show `xwayland: 0` for class `dev.kdmorris.Seshat`
- KDE Plasma: KWin debug console → Windows. The client should be listed under *Wayland Windows*
- GNOME: `Alt+F2` → `lg` → Windows. The window should not be marked as X11 (or use `xlsclients`, which should not list it)

## Manual matrix

| Check | Hyprland | GNOME Wayland | KDE Plasma Wayland |
|---|---|---|---|
| Native launch (`--check-backend`) | ✅ 2026-11-06 | ☐ | ☐ |
| No XWayland (compositor tool) | ✅ 2026-11-06 | ☐ | ☐ |
| Window resize / maximise | ☐ | ☐ | ☐ |
| Fractional scale (125%, 150%) | ☐ | ☐ | ☐ |
| Multiple monitors, move between outputs | ☐ | ☐ | ☐ |
| Keyboard navigation (lists, Enter to open) | ☐ | ☐ | ☐ |
| Theme follows system light/dark | ☐ | ☐ | ☐ |
| Clipboard, DnD, file dialogs | Phase 3+ | Phase 3+ | Phase 3+ |
| Workspace: touchpad horizontal scrolling | ☐ | ☐ | ☐ |
| Workspace: drag files between panels | ☐ | ☐ | ☐ |
| Workspace: Focus Mode at 125%/150% scale | ☐ | ☐ | ☐ |
| Workspace: connectors and cards crisp at 125%/150% scale | ☐ | ☐ | ☐ |
| Workspace: short window falls back to one branch row | ☐ | ☐ | ☐ |

Record the detected backend line from the log with every entry.
