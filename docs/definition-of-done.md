# Definition of Done (spec §18): status

Status as of 2026-11-07, test counts updated 2026-11-09. The spatial workspace has its own status page: [implementation/STATUS.md](implementation/STATUS.md). "Verified" means checked by an automated test or script in this repository, unless marked *manual*.

| Requirement | Status | Evidence |
|---|---|---|
| Builds and runs on Linux | ✅ Verified | `nix build` (tests run in the sandbox), `scripts/check.sh` |
| Runs as a native Wayland client without XWayland | ✅ Verified | `scripts/verify-wayland.sh` (headless Weston, no X server; also on the packaged binary); `hyprctl clients` → `xwayland: 0` |
| Actual display backend verified | ✅ Verified | GdkDisplay type check, logged on every launch; `--check-backend`; the negative control (`GDK_BACKEND=x11`) fails as intended |
| Standard locations resolve correctly | ✅ Verified | XDG user dirs through GLib, localised names, disabled or missing dirs omitted (unit tests) |
| File browsing works | ✅ Verified | listing, navigation, history, breadcrumbs, address bar, search, sort, list and grid views (unit, integration and UI tests) |
| Core file operations work | ✅ Verified | copy, move, rename, delete, Trash, restore, create (in-memory, real-filesystem and UI tests) |
| UI stays responsive | ✅ Verified | operations on worker threads; async listing; 100k-entry folder ≈ 0.3 s per view update ([performance](review/performance.md)) |
| Multiple operations run simultaneously | ✅ Verified | bounded pool of 3, queueing visible over D-Bus (manager and IPC tests) |
| Operation state is centrally managed | ✅ Verified | `OperationManager` is the only state holder; the UI and D-Bus observe its events ([architecture](review/architecture.md)) |
| Progress is exposed through D-Bus | ✅ Verified | `crates/seshat-infrastructure/tests/ipc.rs` |
| An independent application can consume progress | ✅ Verified | `seshat-progress` (no Seshat dependencies), with an end-to-end test against the real service |
| IPC consumers can reconnect and recover state | ✅ Verified | client reconnect and service restart tests; sequence/snapshot procedure |
| Light and dark themes work | ✅ Verified | CSS unit tests; UI screenshots |
| Custom themes work | ✅ Verified | TOML parsing and validation tests; live reload |
| Selected navigation background matches the main content | ✅ Verified | both reference `@seshat_surface` (CSS unit test, every built-in theme) |
| Selected navigation radius is exactly 12px | ✅ Verified | central constant; not overridable by themes (unit tests) |
| Filesystem errors are handled safely | ✅ Verified | partial failures, permission errors, vanished sources, no data loss on failed overwrites, Trash never deletes ([security](review/security.md)) |
| Automated tests pass | ✅ Verified | 248 Rust tests + 51 UI end-to-end checks (`scripts/check.sh --ui`: file operations 20, spatial workspace 31) |
| Documentation is complete | ✅ | see the README's documentation list (all 13 items from spec §15) |

## Not yet verified (manual or environment-dependent)

- **GNOME and KDE Plasma Wayland sessions**, multi-monitor setups and fractional scaling: the manual matrix is in [wayland-verification.md](development/wayland-verification.md). Development used Hyprland and headless compositors.
- **Hardware and session-dependent features:**
  - drag and drop between applications
  - removable devices (mount, eject, unplug while viewing)
  - restoring from the real session Trash
- **D-Bus caller rejection** for a different Unix user (needs a second account).
- **Real-world accessibility** with a screen reader (Orca): labels and keyboard navigation are in place but not yet checked with Orca.

## Decisions still open

- **Project metadata:** the app id is `dev.kdmorris.Seshat` (D-Bus name `dev.kdmorris.Seshat.Operations`). The homepage URL and the developer info in the AppStream data are still missing.
- **License:** GPL-3.0-or-later is a placeholder pending the maintainers' choice.
