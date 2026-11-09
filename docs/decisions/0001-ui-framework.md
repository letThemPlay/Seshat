# ADR 0001 — UI framework

- **Status:** Accepted
- **Date:** 2026-11-06
- **Deciders:** project maintainers
- **Spec references:** §2, §10

## Context

Seshat is a Linux file explorer. It must run as a **native Wayland client**: XWayland does not count. It also needs deep desktop integration (GIO volumes, the freedesktop Trash, portals, DnD, clipboard), full custom theming with exact rules (the shared `surface` token, a 12px selected-navigation radius), accessibility, and long-term maintainability in Rust.

The spec asked us to evaluate four candidates: GTK4 (gtk-rs/Relm4), Iced, Tauri and Slint.

## Evaluation

The table below is based on project documentation and release notes current as of November 2026. GTK4 claims were also checked with the PoC in this repository (see "Proof of concept").

| Criterion (priority) | GTK4 + gtk4-rs + Relm4 | Iced 0.14 | Tauri 2 | Slint |
|---|---|---|---|---|
| Native Wayland (critical) | Yes, the GDK Wayland backend is mature and the default in Wayland sessions; `GDK_BACKEND=wayland` forbids fallback | Yes, through winit/wgpu | Yes, but through WebKitGTK, which has known DMABUF/explicit-sync crashes on some GPUs (Tauri docs: "Linux Graphics Issues") | Yes, through winit or its own backends |
| Linux desktop integration (critical) | Best: GIO (GVolumeMonitor, `trash://`, GVfs network mounts, `AppInfo` launching), portals and the desktop icon themes come built in | Minimal; integrations must be built separately | Through WebKitGTK plus Rust plugins; the file UI would live in HTML | Minimal |
| Rust integration (critical) | Mature bindings (gtk4-rs 0.11, Relm4 0.11 Elm-style) | Native Rust | Rust backend, JS/HTML frontend | Rust plus the `.slint` DSL |
| Custom styling (critical) | Full CSS via `GtkCssProvider`, scoped per class, swappable at runtime | Full control in code, no stylesheet system | Full CSS (it's a browser) | DSL styling, good |
| Filesystem integration (very high) | Async GIO enumerators, file monitors, Trash, mounts | std/tokio only | std/tokio plus IPC to the webview | std/tokio only |
| Async (very high) | GLib main context; `glib::spawn_future_local`; GIO futures | Built-in `Task` | Tokio | Event loop plus `spawn_local` |
| Accessibility (high) | AT-SPI, a mature screen-reader path on GNOME and KDE | Upstream AccessKit integration not shipped yet (only in the libcosmic fork) | Through WebKitGTK's a11y | AccessKit support |
| Drag and drop (high) | Native Wayland data-device; `GdkFileList` in and out | File drop *in* only; no drag *out* to other apps | HTML DnD bridged; dragging files out is limited | Winit-based DnD was still being merged in mid-2026 |
| Performance and memory (high) | Native widgets, recycled `GtkListView` rows (fine for large directories) | Very good GPU rendering | Heaviest: a full web engine process | Very good |
| Packaging (high) | Flatpak GNOME runtime makes this trivial; distro packages exist everywhere | Static binary plus GPU deps | WebKitGTK runtime dependency | Simple |
| Licensing | LGPL-2.1+ (GTK), MIT (gtk4-rs), MIT/Apache (Relm4) | MIT | MIT/Apache | GPLv3, royalty-free (desktop OK), or commercial |
| Long-term maintenance (high) | GNOME-backed, decades-long track record | Pre-1.0, frequent breaking releases | Active | Active, company-backed |
| Development complexity (medium) | Moderate: GObject concepts, but Relm4 helps | Low to moderate | Low for web devs, high for the Rust↔JS split | Moderate |

### Native Wayland details (spec §2.3)

| Question | GTK4 | Iced | Tauri | Slint |
|---|---|---|---|---|
| Native surfaces / no X server needed | Yes (verified, see below) | Yes | Yes | Yes |
| xdg-shell | Yes | Yes (winit) | Yes (GTK) | Yes |
| xdg-decoration | GTK uses client-side decorations and does not request SSD; behaves consistently on GNOME, KDE and Hyprland | winit requests SSD | GTK CSD | winit |
| Fractional scaling (`wp_fractional_scale_v1`) | Yes, since GTK 4.14 | Yes (winit) | Through GTK/WebKit | Yes (winit) |
| Clipboard / DnD | Native `wl_data_device` | Clipboard via `smithay-clipboard`; limited DnD | Through WebKit | Partial |
| File dialogs | `GtkFileDialog` → xdg-desktop-portal | Needs `rfd`/portal | Portal through GTK | Needs `rfd`/portal |
| Multi-monitor / HiDPI | Per-monitor scale, `GdkMonitor` events | winit | GTK | winit |

## Decision

**Use GTK4 through gtk4-rs, with Relm4 for component structure. Use plain GTK CSS for theming. libadwaita is not used for now.**

- GTK4 is the only candidate where everything rated critical is met *and* the GIO stack supplies the volume-monitor, Trash and GVfs features the spec says to reuse rather than reinvent (§4).
- Relm4 gives us an Elm-style update/view separation. That lines up with "presentation observes application state" (§3.5).
- **We don't use libadwaita**, because `AdwStyleManager` and Adwaita's stylesheet impose their own palette. That conflicts with a fully token-driven theme. libadwaita can be revisited for individual widgets later. The `relm4` `libadwaita` feature stays off.

### Theming consequence: provider priority

Seshat installs its stylesheet at `STYLE_PROVIDER_PRIORITY_USER + 1`, which is above a user's `~/.config/gtk-4.0/gtk.css`. Seshat has its own theme system (built-in plus custom themes). Without the higher priority, a desktop-wide GTK theme overrode the mandated rules: on the development machine, a home-manager Orchis-Dark import blanked the file list and replaced the selected-item background. Every Seshat rule is scoped to `.seshat*` classes, so no other surfaces are touched. Users customize Seshat through Seshat themes (Phase 3).

## Rendering architecture

GTK4 renders with GSK (Vulkan, falling back to GL, falling back to Cairo) into a `wl_surface` created by the GDK Wayland backend. The window is an `xdg_toplevel` with client-side decorations. Long lists use `GtkListView`, which recycles row widgets, so a directory with 100k entries costs only as many widgets as are visible.

## Linux dependencies

Build: `pkg-config`, GTK ≥ 4.20 dev files (we use the `v4_20` feature for `gtk-interface-color-scheme`), GLib/GIO ≥ 2.86. Runtime: GTK4, plus GVfs for `trash://` and network mounts, and an icon theme that provides symbolic icons. All of these are in the `flake.nix` devShell.

## Accessibility

GTK4 exposes widgets to AT-SPI. The navigation list and the file list set accessible labels ("Places", "Files"). Keyboard navigation is provided by `GtkListBox`/`GtkListView`. Orca testing is still to do (Phase 6).

## Packaging

Flatpak with the GNOME runtime is the cheapest first target: GTK and GVfs come from the runtime. The manifest needs `--socket=wayland`, `--socket=fallback-x11` *must be omitted* (spec §16), and filesystem access. To be settled in Phase 6.

## Licensing

GTK is LGPL-2.1-or-later and dynamically linked, so it imposes no constraint on Seshat's license. Seshat is licensed GPL-3.0-or-later, confirmed by the maintainer on 2026-11-09 (see `LICENSE`).

## Consequences

- The UI thread runs a GLib main loop. GIO futures are `!Send`, so application ports return `LocalBoxFuture`s. Heavy, CPU-bound operation work (Phase 4) will run on a bounded worker pool and report back through channels.
- Relm4 components are tied to GTK. The domain and application crates stay toolkit-free (enforced by their `Cargo.toml` dependencies).
- `gtk-interface-color-scheme` requires GTK ≥ 4.20.

## Proof of concept

`crates/seshat-presentation` together with `crates/seshat` (`cargo run -p seshat`) provides:

- the main window, a resizable `GtkPaned`, the left navigation pane and the main content pane;
- XDG standard locations, without hardcoded paths, and with disabled or missing directories omitted;
- async directory enumeration through `ListDirectory` → `GioDirectoryReader`, with loading, empty and error states (42 entries in about 10–15 ms on the dev machine, without blocking the UI);
- Light/Dark/System theme switching at runtime, with System following `gtk-interface-color-scheme` live;
- the selected navigation item sharing `@seshat_surface` with the content pane, at a 12px radius. Unit tests in `theme/css.rs` enforce both rules, and the screenshots were checked in both themes.

### Wayland verification results (2026-11-06)

| Environment | Method | Result |
|---|---|---|
| Hyprland (live session) | `seshat --check-backend` (GdkDisplay type) | `wayland (native, wayland-1)`, exit 0 |
| Hyprland (live session) | `hyprctl clients` | `xwayland: 0` |
| Weston 16 headless, no X server | `scripts/verify-wayland.sh` | native backend; `xdg_toplevel` created; check fails without a compositor |
| Forced `GDK_BACKEND=x11` | `seshat --check-backend` | `x11 via XWayland (:0)`, **exit 1** (fallback detected) |

GNOME and KDE Plasma Wayland have not been verified yet. See `docs/development/wayland-verification.md`.

## Sources

- Tauri, "Linux Graphics Issues": https://v2.tauri.app/develop/debug/linux-graphics/
- Slint licensing: https://slint.dev/pricing
- Relm4 0.11 release (April 2026): https://github.com/Relm4/relm4
- Iced 0.14 release notes (December 2025) and the libcosmic AccessKit work: https://github.com/iced-rs/iced
- gtk4-rs 0.11 API docs (crates.io, docs.rs)
