# Testing

Run everything inside `nix develop`. `scripts/check.sh --ui` runs all of the following, and CI runs it on every pull request ([ci.md](ci.md)).

```sh
scripts/check-architecture.sh          # layer dependency rules
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace                 # unit + integration tests
scripts/verify-wayland.sh              # native Wayland verification (headless Weston)
scripts/ui-session.sh scripts/ui/file-operations.sh [artifact-dir]   # UI end-to-end
scripts/ui-session.sh scripts/ui/workspace.sh [artifact-dir]         # spatial workspace end-to-end
```

## Automated tests

| Layer | Where | What |
|---|---|---|
| Domain | `crates/seshat-domain` | lifecycle transitions, progress, locations, entities; the **workspace**: branching, shared ancestors, navigation, jump list, Focus Mode as presentation only, persistence round trips, and invariants checked after every step of 40 randomised 400-step operation sequences |
| Application | `crates/seshat-application` | navigation, view arrangement, validation, and the **operation manager on an in-memory filesystem** (`MemoryFs`: two devices, a Trash, quotas, failure injection, a gate for deterministic cancellation and queueing tests) |
| Workspace | `crates/seshat-application/src/workspace/tests.rs` | the **workspace service** against a fake shell that answers `Load` effects, so reloads are counted exactly; tests are named after the spatial spec §16 criteria (`spec_16_*`), including cross-branch copies and moves run through the real operation manager; keymap, mode machine and command parser tests in `interaction/` |
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

The two scripts:

- `file-operations.sh`: copy, paste, conflicts, create, rename, Trash, permanent delete, a large background copy after the window closes, and D-Bus publication.
- `workspace.sh`: the spatial spec's definition of done (§19). It covers branches, the side-by-side layout (rows, columns, cards, connectors, and the single-row fallback in a short window), Focus Mode navigation and branch switching, a cross-branch copy with confirmation, and the workspace restored after a restart. It reads workspace state from the `workspace state` debug line, which lists locations, never file listings.

Notes for writing scripts:

- Send the shifted character for Shift chords (`chord ctrl shift N`), as a real keyboard would.
- Headless Sway doesn't refocus a parent window after its dialog closes; call `focus_main`.
- Navigate with absolute paths in the address bar (`type_text "$H/Notes"`).

## Manual checks

Everything automation can't cover is in one step-by-step checklist, [manual-verification.md](manual-verification.md), with fixtures from `scripts/manual/verification-fixtures.sh`. It covers:

- real GNOME and KDE Plasma sessions, scaling and multiple monitors
- drag and drop and the clipboard with other applications
- removable devices (plug in, mount, eject, unplug while viewing)
- the real session Trash and restoring from it
- Orca, and D-Bus requests from another user

Results go into the matrix in [wayland-verification.md](wayland-verification.md).

## Performance measurements

```sh
cargo test --release -p seshat-infrastructure --test perf -- --ignored --nocapture
SESHAT_RELEASE=1 scripts/ui-session.sh <script>   # UI timings: "panel view updated" debug lines
cargo test --release -p seshat-application --test workspace_perf -- --ignored --nocapture   # workspace
cargo test --release -p seshat-presentation layout_scaling -- --ignored --nocapture        # layout
SESHAT_RELEASE=1 SESHAT_STALL_MONITOR=1 scripts/ui-session.sh scripts/ui/workspace-perf.sh   # end to end
```

Results and their interpretation are in [review/performance.md](../review/performance.md).
