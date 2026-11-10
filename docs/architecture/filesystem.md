# Filesystem integration

All filesystem access goes through ports in `seshat-application::ports`. GIO implements them in `seshat-infrastructure::filesystem`. Presentation never calls GIO for I/O.

| Port | Implementation | Purpose |
|---|---|---|
| `DirectoryReader` | `GioDirectoryReader` | Async listing of a `Location` |
| `DirectoryWatcher` | `GioDirectoryWatcher` | Live change notifications (`GFileMonitor`) |
| `DeviceMonitor` | `GioDeviceMonitor` | Mounts and mountable volumes (`GVolumeMonitor`), mount and eject |
| `StandardLocations` | `XdgStandardLocations` | Home, XDG user directories, Trash |
| `Launcher` | `GioLauncher` | Open files with the default app (uses the OpenURI portal when sandboxed) |

## Locations

`Location` is either `Path(PathBuf)` or `Trash(PathBuf)`. A Trash location is a path *relative to the Trash root*, because the freedesktop Trash spans one directory per filesystem. GIO's `trash:///` (GVfs) merges those directories, and `gio_file()` is the only place that maps a domain location to a GIO URI.

## Listing

- Only metadata attributes are queried (`standard::*`, `time::modified`, `trash::*`). File contents are never read.
- `NOFOLLOW_SYMLINKS`: links are reported as `EntryKind::Symlink`. For links only, one extra stat of the target records `to_directory`, so links to folders can be browsed and broken links can't. A link reports no size.
- Entries are fetched in batches of 256, which keeps memory bounded per round trip and keeps the main loop responsive in very large directories.
- Names stay `OsString`. `display_name` is for display only: the lossy filename, or the GVfs display name inside Trash (older GVfs encodes internal Trash names).
- Errors map to `ListingError::{NotFound, PermissionDenied, NotADirectory, Unavailable, Other}`. Each has a user-facing message in the UI.

## Trash (browsing)

Top-level Trash items carry `TrashInfo { original_path, deleted_at }` from `trash::orig-path` and `trash::deletion-date`. The UI shows the original folder and the deletion date. If GVfs isn't running, the Trash location reports `Unavailable` rather than failing silently.

Moving files to Trash, restoring and permanent deletion are file *operations* (spec §8.1). They will be implemented on the Operation Manager in Phase 4, so they get progress, cancellation and D-Bus visibility. A failed trash must never fall back to deletion.

## Live updates

Every panel whose listing is cached (the 16 most recently used, decision D13) is watched. A panel is watched *before* it is listed, so changes made between the two aren’t missed. Events are classified as:

- `Modified`: created, deleted, moved, renamed, attribute changed, or a write finished. Reloads are debounced (300 ms), so bursts like extracting an archive cause one reload. Plain `Changed` events, which fire repeatedly during a write, are ignored until `ChangesDoneHint`.
- `Removed`: the watched directory itself was deleted, moved away or unmounted. The panel and the panels below it stay in the workspace, marked unavailable, with Retry, Go to Parent and Close Branch actions (decision D14). No branch is closed for you.

Each watch and listing carries a generation; events and listings from superseded ones are dropped.

Reloads keep each panel’s cursor (or move it to the nearest item), selection (minus vanished items) and scroll position. Filtering, search and sorting run in memory on the last listing (`view::arrange`), so changing them never re-reads the disk.

## Devices

`GioDeviceMonitor` reports:

- mounted, non-shadowed mounts that have a local path (including GVfs FUSE paths for network shares), and
- unmounted volumes that can be mounted.

GIO's user-visibility rules already hide system mounts (`/`, `/boot`, …). A device's id comes from its volume (UUID, then unix device, then label), so it stays stable across mount and unmount.

- **Mount:** selecting an unmounted device mounts it and opens its root. No interactive `GMountOperation` is wired up yet, so encrypted volumes report an error instead of prompting for a passphrase.
- **Eject/unmount:** panels on the device stop watching first, so Seshat doesn’t keep it busy; they stay in the workspace, marked unavailable. Success is reported as "can be removed safely".
- **Disappearance:** when the device list changes, `vanished_roots(before, after)` lists the mount points that went away, and every panel on them is marked unavailable (not closed).

## Testing

- Unit tests cover `Location` arithmetic, history, breadcrumbs, address parsing, device-loss detection and watcher event classification.
- Integration tests (`crates/seshat-infrastructure/tests/`) use real temp directories: files, directories, hidden files, symlinks (to a directory, to a file, broken), non-UTF-8 names, 1,000 entries, permission errors, a live file-creation event, and deletion of the watched directory.
- The Trash test is **read-only** against the session Trash, and skips when GVfs is unavailable.
- Device hotplug can't be automated without hardware. Verify it manually by plugging in a USB stick: it should appear in the sidebar, clicking it should mount it, eject should leave the device and report it safe to remove, and unplugging it while viewing it should leave its panels in place, marked unavailable (decision D14). The full procedure is in [manual-verification.md](../development/manual-verification.md), section 8.
