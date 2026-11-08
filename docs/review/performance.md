# Performance review (Phase 6, 2026-11-07)

These are release builds on the development machine (btrfs, NVMe). To reproduce:

```sh
cargo test --release -p seshat-infrastructure --test perf -- --ignored --nocapture
SESHAT_RELEASE=1 scripts/ui-session.sh <script>    # UI timings ("view updated" debug log lines)
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
