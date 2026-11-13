# Linux and Wayland requirements

Native Wayland operation is a release requirement ([original specification](../specifications/0-INITIAL-PRODUCT-SPEC.md) §10). An application running through XWayland does not meet it.

## Decisions in force

- **Toolkit:** GTK4 through gtk4-rs and Relm4, without libadwaita. See [ADR 0001](../decisions/0001-ui-framework.md).
- **Runtime verification:** the backend is verified from the concrete `GdkDisplay` type, never from environment variables alone. It is logged on every launch, and `seshat --check-backend` exits non-zero on X11 or XWayland.
- **Development:** the dev shell sets `GDK_BACKEND=wayland`, so GTK fails instead of silently falling back to X11.
- **Packaging:** packages must not force X11. See [ADR 0003](../decisions/0003-packaging.md) and [packaging.md](../packaging.md).
- **Portals:** opening files uses the OpenURI portal when sandboxed, and file dialogs go through the portal (via `GtkFileDialog`).
- **Clipboard and drag and drop** use GTK's native Wayland data-device support.

## The spatial workspace

The workspace adds only standard GTK widgets: a horizontal `ScrolledWindow`, boxes, list and grid views, buttons and popovers. It adds no protocol requirements.

- **Horizontal scrolling** (touchpad and wheel) comes from GTK's scrolled window.
- **Panel reveal animation** uses frame-clock tick callbacks, which are supported on every Wayland compositor.
- **Keyboard handling** is a capture-phase `EventControllerKey` on the toplevel. It works with the compositor's keymap through GDK and needs no global shortcuts or other compositor-specific protocols.
- **The saved workspace** lives in `$XDG_STATE_HOME/seshat` (XDG Base Directory), next to the configuration in `$XDG_CONFIG_HOME/seshat`.

## Fuzzy Find

Fuzzy Find also adds no protocol requirements.

- **The overlay** is a `gtk::Overlay` child inside the main window, not a separate surface, so it needs no layer-shell or positioning protocol. Its scope buttons are ordinary toggle buttons.
- **Its keys** (`Ctrl+P`, and `Tab`, `Shift+Tab`, `Alt+Enter` and `Alt+1`…`Alt+3` inside it) go through the same toplevel key handling, with no global shortcuts.
- **Opening a file result** uses the existing launcher, and so the OpenURI portal when sandboxed.
- **Searching** reads folders on a background thread with ordinary filesystem calls; it doesn't follow symbolic links or cross onto other mounts, and it needs no portal or extra permission beyond those the file manager already has.

## Verification

- **Automated:** `scripts/verify-wayland.sh` (headless Weston, no X server) passes with the spatial workspace. The UI end-to-end tests run on a headless Sway compositor.
- **Manual:** the GNOME, KDE, scaling and multi-monitor matrix is in [wayland-verification.md](../development/wayland-verification.md). Touchpad horizontal scrolling and drag and drop between panels are added to it.
