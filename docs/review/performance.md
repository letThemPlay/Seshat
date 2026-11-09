# Performance review (Phase 6, 2026-11-07)

These are release builds on the development machine (btrfs, NVMe). To reproduce:

```sh
cargo test --release -p seshat-infrastructure --test perf -- --ignored --nocapture
SESHAT_RELEASE=1 scripts/ui-session.sh <script>    # UI timings ("panel view updated" debug log lines)
```

## Results

| Scenario | Result |
|---|---|
| List 100,000 entries (GIO, async, metadata only) | 0.42–0.49 s, off the UI thread |
| Sort 100,000 by name (natural) | **427 ms → 61 ms** after the fix below |
| Sort 100,000 by modified date | 318 ms → 52 ms |
| Search 100,000 (1,111 matches) | 4–7 ms |
| UI update with 100,000 rows (sort and GTK model swap) | ~320 ms |
| Memory: idle / 100,000 rows shown | 92 MiB / 220 MiB |
| Copy 512 MiB on tmpfs | Seshat 2,624 MiB/s vs `cp` 2,446 MiB/s |
| Progress events for that copy | 2 (throttled to ≤ 10/s per operation) |

## Changes made

- **Natural sort:** names are case-folded once per entry instead of on every comparison, and comparison is byte-wise without allocating. That's 7× faster.
- **Adaptive live-reload debounce:** at least 300 ms and at least 3× the last rebuild, capped at 3 s. A huge folder that changes constantly can't keep the UI busy rebuilding.

## Already in place

- **GTK `ListView`/`GridView` recycle rows**, so the widget count is proportional to what's visible, not to the folder size.
- **Sorting, filtering, search and the hidden-files toggle work on the in-memory listing,** never touching the disk again.
- **Copies** use `copy_file_range` in 8 MiB chunks: kernel-side copying, with cancellation between chunks.
- **File operations run on bounded worker threads.** The UI thread only receives events.

## Known characteristic

Swapping 100,000 rows into the GTK model takes about 260 ms, which is most of the ~320 ms update. Incremental model updates (diffing instead of replacing) would remove it. This only matters for very large folders, so it's left as a follow-up.

# Spatial workspace performance run (2026-11-09)

Covers the spatial file manager specification §14 and §16.8, with the targets and scale approved before the run (R1–R4). Release builds on the development machine: 16 CPUs, btrfs on NVMe, headless Sway for the UI. Treat the numbers as regression references for this machine, not guarantees.

```sh
cargo test --release -p seshat-application --test workspace_perf -- --ignored --nocapture
cargo test --release -p seshat-presentation layout_scaling -- --ignored --nocapture
cargo test --release -p seshat-infrastructure --test perf workspace_save_and_load -- --ignored --nocapture
SESHAT_RELEASE=1 SESHAT_STALL_MONITOR=1 scripts/ui-session.sh scripts/ui/workspace-perf.sh [artifact-dir]
```

The UI script restores a seeded workspace of **50 branches and 211 panels** (1,000 files in each branch tip), then switches branches, toggles Focus Mode, moves between panels, and reloads a **100,000-entry folder** while keys are pressed. It summarises Seshat's debug timing lines (counts and durations, never file names). `SESHAT_STALL_MONITOR=1` logs every main-loop gap over 100 ms, which is how responsiveness is measured.

## Workspace logic and layout (headless)

| Operation | 50 branches | 200 branches | 1,000 branches |
|---|---|---|---|
| Branch switch (domain) | 2.9 µs | 29 µs | 630 µs |
| Branch switch (service, with effects) | 14 µs | 61 µs | 649 µs |
| Activate any panel | 4.2 µs | 21 µs | 139 µs |
| `h` + `l` | 0.16 µs | 0.22 µs | 0.26 µs |
| Focus Mode toggle | 12 ns, 0 folder reads | 13 ns | 12 ns |
| Layout of the side-by-side grid | 8 µs | 37 µs | 1.0 ms |
| Workspace-wide selection (50 panels × 100 items) | 176 µs | 212 µs | 1.4 ms |

Branch listing and layout grow with the square of the branch count, because each leaf's branch is found by a linear search. That is well inside the targets at the approved scale and still under 1 ms at 1,000 branches, so it was left alone. A tip-to-branch index would make it linear if workspaces ever get that large.

## In the real app (50 branches, 211 panels)

| Measurement | Result | Target (R1) |
|---|---|---|
| Render, branch switch (100 switches) | median 0.06 ms, p95 5.0 ms, max 49 ms | p95 < 16 ms, switch < 50 ms ✅ |
| Render, Focus Mode toggle (20) | p95 0.44 ms, max 0.72 ms, **0 folder reads** | < 16 ms, 0 reads ✅ |
| Render, `h` and `l` | p95 1.1 ms, max 4.2 ms | ✅ |
| Render, `j` in a 1,000-row panel | p95 0.32 ms | ✅ |
| Save, 211 panels | **25 ms → 0.35 ms** on the UI thread after the fix below; the file is on disk ~25 ms later | < 20 ms ✅ |
| Save and restore, 1,000 branches (3,011 panels, 700 KB) | 16 ms to queue, 25 ms to load | — |
| Memory | 115 MiB restored, 128 MiB after visiting every branch, 258 MiB with the 100,000-entry folder | — |
| 100,000-row panel, first display | 141 ms model swap | exempt (R3) |
| 100,000-row panel, live reloads while typing | 5 reloads, 250–270 ms model swap each; 5 main-loop stalls, longest 365 ms | exempt (R3), see below |
| Folder reads per round of 50 branch switches | 200, on the first **and** on later rounds | see below |

The max render time of a branch switch (49 ms) is the switch that swaps in the most rows. The median is under 0.1 ms.

## Fix made

**Saving no longer blocks the UI.** A save took about 25 ms on the UI thread, almost all of it the `fsync` of the atomic write, which on btrfs waits for pending metadata. `TomlWorkspaceStore` now encodes on the caller's thread and writes on a writer thread. Saves made while one is being written are coalesced, so only the latest is written. `flush()` waits for the writer at shutdown and before `load()`, so nothing is lost and a restore never reads a file that is about to be replaced.

## Open findings

1. **Repeated folder reads when cycling through many branches.** Each branch shows about 4 full panels, so 50 branches touch about 200 panels, but only 16 listings are cached (D13). Every switch re-reads the panels it shows, in the background: the UI stays fast, and nothing reloads while you stay within ~4 branches. A larger cache or a byte budget would change D13, so it is left for a product decision.
2. **Responsiveness during live reloads of a 100,000-entry folder.** Each reload replaces the whole GTK model (250–270 ms) and the UI pauses for up to 365 ms. Under R3 this is exempt and documented. Incremental model updates (applying only the changed rows) would remove it.
