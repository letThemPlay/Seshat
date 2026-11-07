# Testing

Run everything inside `nix develop`. `scripts/check.sh --ui` runs all of the following.

```sh
scripts/check-architecture.sh          # layer dependency rules
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace                 # unit + integration tests
scripts/verify-wayland.sh              # native Wayland verification (headless Weston)
scripts/ui-session.sh scripts/ui/file-operations.sh [artifact-dir]   # UI end-to-end
```

## Automated tests

| Layer | Where | What |
|---|---|---|
| Domain | `crates/seshat-domain` | lifecycle transitions, progress, locations, entities |
| Application | `crates/seshat-application` | navigation, view arrangement, validation, and the **operation manager on an in-memory filesystem** (`MemoryFs`: two devices, a Trash, quotas, failure injection, a gate for deterministic cancellation and queueing tests) |
| Infrastructure | `crates/seshat-infrastructure/tests` | real temp directories: listing, watcher events, the manager with `LocalFileSystem` (permissions, mtime, symlinks, no-replace rename, permission errors, cancelling large copies); **D-Bus** on a private `dbus-daemon`: discovery, snapshots, ordered signals without gaps, cancellation, structured errors, queueing, client reconnects, name release and no hijacking |
| IPC consumer | `crates/seshat-progress/tests` | runs the example client binary against the real service on a private bus: discovery, completion, service stop and restart |
| Presentation | unit tests | theme rendering and validation, clipboard parsing, action rules, progress text |

The tests never touch the real Trash (GIO would use the session's). Trash behaviour is covered by `MemoryFs` and by the UI test.

## UI end-to-end tests

`scripts/ui-session.sh` runs a test script against the real app, isolated from your desktop:

- **A separate compositor:** a headless Sway with its own `XDG_RUNTIME_DIR`. Weston can't inject input, so the UI tests use Sway; the Wayland verification stays on Weston.
- **A scrubbed environment:** `env -i`, so no `DISPLAY`, no session D-Bus, and a throwaway `HOME`, `XDG_CONFIG_HOME` and `XDG_DATA_HOME` (and therefore Trash).
- **Input and screenshots** (`wtype`, `grim`) go only to that compositor. Every helper checks that the socket lives in the session directory.
- **A read-only filesystem:** Seshat runs under `bubblewrap` with everything read-only except the session directory, so a test that goes astray cannot modify real files. The harness proves this with a probe write before every run.

Test scripts are sourced with helpers: `launch`, `type_text`, `press`, `chord ctrl shift N`, `swaycmd`, `focus_main`, `shot <name>`, `check <description> <command…>`, `wait_exit`, `window_open`, `not`. Screenshots and logs land in the artifact directory. A run fails if any check fails, including the final check that the log contains no `panicked` or `ERROR` lines.

Notes for writing scripts:

- Send the shifted character for Shift chords (`chord ctrl shift N`), as a real keyboard would.
- Headless Sway doesn't refocus a parent window after its dialog closes; call `focus_main`.
- Navigate with absolute paths in the address bar (`type_text "$H/Notes"`).

## Manual checks

The Wayland matrix (GNOME, KDE, scaling, multi-monitor) is in `wayland-verification.md`. Things automation can't cover yet:

- drag and drop between applications
- removable devices (plug in, mount, eject, unplug while viewing)
- the real session Trash and restoring from it

## Performance measurements

```sh
cargo test --release -p seshat-infrastructure --test perf -- --ignored --nocapture
SESHAT_RELEASE=1 scripts/ui-session.sh <script>   # UI timings: "view updated" debug lines
```

Results and their interpretation are in [review/performance.md](../review/performance.md).
