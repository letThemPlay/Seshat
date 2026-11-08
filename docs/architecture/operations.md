# File operations

All file changes go through the **`OperationManager`** (`seshat-application::operations`). It is the single source of truth for operation state. The UI and, from Phase 5, D-Bus consumers both *observe* it; neither performs file operations itself.

```text
UI / IPC ──submit(OperationRequest)──▶ OperationManager ──▶ worker thread ──▶ FileSystem port
                                          │  state + sequence                  (LocalFileSystem:
   ◀── OperationEvent (sequence, kind, ───┘                                     std::fs, rustix, GIO)
       operation snapshot)
```

## Requests and validation

`OperationRequest` covers Copy, Move, Rename, Delete (permanent; paths and Trash items), Trash, Restore, CreateDirectory and CreateFile. `validate()` runs before anything is created, and an invalid request produces no operation and no events. It rejects:

- empty item lists and relative paths, or paths containing `..`
- acting on `/`
- copying or moving a folder into itself
- invalid names (empty, `.`, `..`, `/`, NUL, longer than 255 bytes)
- non-top-level Trash items for Restore

## Lifecycle and events

```text
Queued → Started → InProgress → Completed | Failed | Cancelling
Cancelling → Cancelled | Completed | Failed
```

Transitions are enforced by the domain `Operation` entity. Every change emits an `OperationEvent { sequence, kind, operation }`:

- `sequence` is strictly increasing across all operations.
- Lifecycle events are never dropped. `Progress` events are throttled to one per operation every 100 ms.
- An operation cancelled from a conflict dialog still emits `Cancelling` before `Cancelled`.

**Snapshot procedure** (for observers, including D-Bus clients in Phase 5):

1. `subscribe` first, buffering events.
2. Take a `snapshot()`. It returns the active operations and the sequence number they correspond to.
3. Discard buffered events with `sequence <= snapshot.sequence`, then apply the rest.
4. If a gap in `sequence` appears, take a new snapshot.

This is consistent because state changes and event delivery happen under the same lock order (state, then events), and snapshots take both locks.

## Execution

- **Concurrency is bounded:** at most `MAX_CONCURRENT_OPERATIONS` (3) worker threads run at once; the rest queue. A panicking worker fails its operation instead of taking the app down.
- **Planning:** copies and cross-device moves are scanned first. That gives accurate totals for files and bytes, so progress is determinate, and lets free space be checked before writing (`statvfs`). Same-device moves are single `rename`s.
- **Copying** uses `copy_file_range` in 8 MiB chunks (via `std::io::copy`), checking cancellation between chunks. It preserves permissions and modification time. Symlinks are recreated, never followed. Special files are reported and skipped.
- **Cancellation** is cooperative. A queued operation ends immediately. A running one stops at the next chunk or item, and partially written files are removed.

## Safety rules

- **Overwriting never destroys the original on failure.** The new content goes to a hidden temporary sibling (`.name.seshat-part-…`), which replaces the target with one atomic `rename` only once complete.
- **Rename never overwrites.** It uses `renameat2(RENAME_NOREPLACE)`, falling back to check-then-rename on filesystems without support.
- **Moved items' sources are removed only after a complete copy.** Folders are removed only when everything inside them was moved.
- **Cleanup never deletes files Seshat didn't create.** If the target appeared concurrently (`AlreadyExists`), it is left alone.
- **Trash never falls back to deletion.** Items the Trash refuses are reported ("The item was not deleted"), and the UI then offers permanent deletion as an explicit, confirmed choice.
- **Deletion does not follow symlinks.**
- **Partial failures are recorded per item** (`Operation::failures`) and the operation continues. An operation in which every item failed ends as `Failed`.

## Conflicts

When a target exists, the worker asks the `ConflictResolver`. The choices are Replace (Merge for folders), Skip, Keep Both (`name (2).ext`) or Cancel, optionally applied to all later conflicts. The UI resolver shows a dialog and blocks the worker until it gets an answer. Cancelling the operation unblocks it within 100 ms. Without a UI resolver, conflicts are skipped and nothing is overwritten.

## Background behaviour (current limitation)

The manager runs inside the UI process. Closing the window while operations are active hides it and keeps the process alive until they finish, then it exits. A conflict brings the window back. Killing the process still aborts running operations, and the temporary files from interrupted overwrites are left behind as hidden `.seshat-part-*` files. Because the manager is only reachable through application-layer types, moving it into a separate D-Bus-activated service later needs no domain or application changes.

## Adding a new file operation

1. **Domain:** add a variant to `OperationKind` (with `id()` and `verb()`).
2. **Application:**
   - Add an `OperationRequest` variant and extend `kind()`, `sources()`, `destination()` and `validate()`.
   - Implement the algorithm in its own module, taking a `Job`. Use `job.begin(files, bytes)`, `set_current`, `item_done` / `item_failed` / `skipped`, `check_cancel()` between items, and `job.resolve()` for conflicts.
   - Dispatch it in `job::execute`.
   - If it needs a new filesystem primitive, add it to the `FileSystem` port.
3. **Infrastructure:** implement any new port method in `LocalFileSystem`, plus the test double `MemoryFs`.
4. **Tests:** add in-memory tests in `operations/tests.rs` (lifecycle, conflicts, failures, cancellation) and a real-filesystem test in `crates/seshat-infrastructure/tests/operations.rs`.
5. **UI:** add an action in `actions.rs` (with its `ActionContext` rule), a menu entry, optionally a shortcut, and a handler in `app/file_ops.rs` that builds the request and calls `submit`. The progress panel picks it up automatically from the events.
