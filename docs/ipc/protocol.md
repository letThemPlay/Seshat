# D-Bus protocol (version 1)

Seshat publishes its file operations on the **session bus**, so other applications (panels, status bars, scripts) can show progress and cancel operations. The protocol uses typed D-Bus values only, so any language with a D-Bus binding can use it.

| | |
|---|---|
| Bus | session bus |
| Bus name | `com.example.Seshat.Operations` |
| Object path | `/com/example/Seshat/Operations` |
| Interface | `com.example.Seshat.Operations` |

These names are placeholders until the project's reverse-DNS identity is decided. They are defined once, in `seshat-infrastructure::ipc`. The bus name is deliberately different from the GTK application id, which GApplication owns.

The service exists while Seshat runs, including while the window is closed but operations are still finishing. There is no D-Bus activation yet, because operations only exist while Seshat runs.

## Methods

| Method | Signature | Description |
|---|---|---|
| `GetProtocolVersion` | `() → u` | `1`. Incompatible changes bump it. |
| `GetActiveOperations` | `() → (t a(OperationInfo))` | A snapshot: the sequence number it reflects, and every unfinished operation (queued, started, in progress or cancelling). |
| `GetOperation` | `(s id) → OperationInfo` | One operation, active or recently finished. Seshat keeps the last 64 finished operations. |
| `CancelOperation` | `(s id) → ()` | Requests cancellation. The operation then reports `OperationCancelling` and later `OperationCancelled`, or `OperationCompleted` if it finished first. |

## Signals

Each signal has the arguments `(t sequence, OperationInfo operation)`:

`OperationQueued`, `OperationStarted`, `OperationProgress`, `OperationCancelling`, `OperationCompleted`, `OperationFailed`, `OperationCancelled`.

- `sequence` increases by exactly 1 for every event of every operation, within one run of Seshat.
- Lifecycle signals (everything except `OperationProgress`) are never dropped.
- `OperationProgress` is sent at most every 100 ms per operation. It is also sent immediately for important changes: a per-item failure, or waiting for or receiving a conflict decision.
- `OperationProgress` covers any non-lifecycle change: counts, the current item, `awaiting_decision`, `failure_count`.

## `OperationInfo`

Signature `(sssasussttbttbdbtttsu)`. The field order is the contract.

| # | Field | Type | Meaning |
|---|---|---|---|
| 1 | `id` | `s` | Operation id (hyphenated UUID) |
| 2 | `kind` | `s` | `copy`, `move`, `rename`, `delete`, `trash`, `restore`, `create_directory`, `create_file` |
| 3 | `status` | `s` | `queued`, `started`, `in_progress`, `cancelling`, `completed`, `failed`, `cancelled` |
| 4 | `sources` | `as` | First 32 items. Paths, or `trash:///name` for Trash items |
| 5 | `source_count` | `u` | Total number of items |
| 6 | `destination` | `s` | Target folder (the new path for `rename`), or empty |
| 7 | `current` | `s` | Item being processed, or empty |
| 8–10 | `files_completed`, `files_total`, `files_total_known` | `t t b` | Item counts. `files_total` is meaningful only if known |
| 11–13 | `bytes_completed`, `bytes_total`, `bytes_total_known` | `t t b` | Byte counts (copies and cross-device moves) |
| 14 | `percentage` | `d` | 0–100, based on bytes when known, otherwise items. **-1 when the total is unknown**; never an estimate |
| 15 | `awaiting_decision` | `b` | Waiting for the user to resolve a conflict in Seshat |
| 16–18 | `queued_at_usec`, `started_at_usec`, `elapsed_ms` | `t t t` | Microseconds since the Unix epoch (0 = not yet), and run time |
| 19 | `error` | `s` | Why the whole operation failed, or empty |
| 20 | `failure_count` | `u` | Items that could not be processed. Their paths stay in Seshat |

Paths are UTF-8 strings. Linux filenames need not be valid UTF-8, so invalid sequences appear as U+FFFD. Treat these strings as display text, not as addresses for file access.

## Errors

The error names are `com.example.Seshat.Operations.Error.*`:

| Name | When |
|---|---|
| `NotFound` | Unknown id, or finished long ago |
| `AlreadyFinished` | `CancelOperation` on a finished operation |
| `InvalidArgument` | Malformed id (not a UUID, too long) |
| `AccessDenied` | The caller is not the user running Seshat |

## Staying consistent: subscribe, snapshot, apply

To avoid races between the snapshot and the signals:

1. **Subscribe first.** Add match rules for the interface's signals, and for `org.freedesktop.DBus.NameOwnerChanged` with `arg0='com.example.Seshat.Operations'`. Buffer what arrives.
2. **Take a snapshot** with `GetActiveOperations()`, which returns `(S, operations)`.
3. **Apply buffered and later signals** with `sequence > S`, and ignore those with `sequence ≤ S` (the snapshot already includes them).
4. **On a gap** (`sequence > last + 1`), take a new snapshot (step 2).
5. **On `NameOwnerChanged`:**
   - If the new owner is empty, Seshat stopped. Drop your state and wait.
   - If the new owner is non-empty, a new instance started, and its sequence numbers start again at 1. Take a new snapshot.
6. **On a lost bus connection,** reconnect and start again at step 1.

Operations are removed from the active set by the terminal signals (`OperationCompleted`, `OperationFailed`, `OperationCancelled`).

## Security

- Session bus only, which already limits access to the user's session.
- Every method also checks the caller's Unix user (`GetConnectionUnixUser`) against Seshat's own user and rejects others with `AccessDenied`. This protects against shared or misconfigured buses.
- Ids are validated before use. Cancellation goes through the same manager API as the UI, so IPC cannot bypass application validation.
- Messages are kept small:
  - sources are capped at 32
  - only failure counts are exposed, not the paths of failed items
  - no file contents are ever exposed
- Only one instance can own the name. A second instance does not replace or queue behind the first; it runs without publishing.

## Trying it

```sh
# Watch the signals
dbus-monitor --session "interface='com.example.Seshat.Operations'"

# Snapshot
busctl --user call com.example.Seshat.Operations /com/example/Seshat/Operations \
    com.example.Seshat.Operations GetActiveOperations

# Example client (crates/seshat-progress)
cargo run -p seshat-progress                 # follow until interrupted
cargo run -p seshat-progress -- --exit-when-idle
```

`crates/seshat-progress` is a complete consumer that deliberately doesn't depend on Seshat's crates. It implements the procedure above, including gap detection and surviving service restarts. Illustrative output:

```text
· 0 active operation(s)
[3f2a9c1e] copy Photos → Backup                         37.5%  45 of 120 items
[3f2a9c1e] copy Photos → Backup                      ✔ completed
· Seshat stopped; waiting for it to return
```
