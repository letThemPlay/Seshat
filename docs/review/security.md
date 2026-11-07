# Security review (Phase 6, 2026-11-07)

**Scope:** filesystem operations, input from other processes (clipboard, drag and drop, D-Bus, theme and settings files), privilege-related metadata, logging, and dependencies.

## Findings fixed

| # | Issue | Impact | Fix | Test |
|---|---|---|---|---|
| 1 | Copy/move destinations weren't resolved, so a symlinked alias could defeat the "into itself" check | Copying `/a` into `/x` (where `/x` links to `/a/sub`) recursed until the disk was full. Pasting into a symlinked folder also failed | Destinations are canonicalized; "into itself" is re-checked against real paths at run time | `copying_a_folder_into_an_alias_of_itself_is_refused`, `copy_into_symlinked_folder_and_alias_refusal` |
| 2 | Copies kept setuid/setgid bits | A copied file could carry privilege bits | Files keep `0o777`, folders `0o1777` (like `cp` when ownership changes) | `copies_drop_setuid_and_setgid` |
| 3 | Recursive operations checked a folder with `lstat`, then listed it with a call that follows symlinks | A folder swapped for a symlink in between (e.g. in a shared folder like `/tmp`) made a delete empty the link's target | Listing uses `O_NOFOLLOW` and refuses symlinks | `listing_refuses_to_follow_a_symlinked_folder` |
| 4 | A second instance could take over the D-Bus name (zbus allows replacement by default) | Observers silently switched to another process | No replacement and no queueing | `service_restart_releases_and_reclaims_the_name` |
| 5 | GTK signal handlers could panic during teardown | Process abort at exit | Non-panicking sends; UI tests fail on any panic | UI check "no panics or errors in the log" |

## Verified as safe

- **Overwrites** go through a temporary sibling created with `O_EXCL` and mode `0600`, then an atomic rename. A failed copy never damages the original, and Seshat never deletes a file it didn't create.
- **Rename** never overwrites (`RENAME_NOREPLACE`).
- **The Trash never falls back to deletion.** Permanent deletion always needs an explicit confirmation, and Cancel is the default button.
- **Deletion doesn't follow symlinks.** Symlinks are recreated, never followed, when copying.
- **External input** is validated by the same `OperationRequest::validate`:
  - D-Bus checks the caller's Unix user and validates ids.
  - Clipboard and drop data must be absolute local paths, and clipboard reads are capped at 4 MiB.
- **Theme files** are capped at 64 KiB and parsed into typed values. The font family is sanitised, so CSS can't be injected.
- **Settings** are written atomically with mode `0600`.
- **Opening files** goes through GIO's default-handler lookup or the portal. Nothing goes through a shell.
- **Logs** contain paths of failed items and locations, never file contents or full listings.
- **Dependencies:** `cargo audit` (RustSec, 1,422 advisories) found 0 vulnerabilities and 0 warnings across 143 crates.

## Residual risks

- **A symlink race narrower than #3 remains.** If a folder being deleted is swapped for a symlink *after* it was listed, removing a child by path follows the swapped component. Closing this fully requires file-descriptor-relative operations throughout (`openat`/`unlinkat`) and a different `FileSystem` port shape. Planned.
- **Rename has a check-then-rename fallback** on filesystems without `RENAME_NOREPLACE`. The time window is small, and it only affects the user's own files.
- **D-Bus signals carry paths.** Any process of the same user on the session bus can see them; the session bus is per-user. Failure paths are never exposed, only counts.
- **The "different user" rejection path** is implemented but untested, because it needs a second account.
