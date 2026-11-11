# Implementation status

Status as of 2026-11-11, branch `refactor-file-actions`.

"Verified" means checked by an automated test or script in this repository, unless marked *manual*. The original specification's status is kept in [definition-of-done.md](../definition-of-done.md), and it still holds: every original check passes unchanged, except the UI test now starts searches with `/` (decision D1).

**Checks:** `scripts/check.sh --ui` passes. That covers fmt, clippy with `-D warnings`, the architecture rules, 341 Rust tests (up from 295 before backward pruning, 165 originally), native Wayland verification, and both sandboxed UI tests: file operations (20 checks) and the spatial workspace (63 checks, including backward pruning).

## Spatial file manager specification

| § | Requirement | Status | Evidence / notes |
|---|---|---|---|
| 3.1, 3.4 | Opening a directory creates or activates a child; never replaces a branch | ✅ Verified | `spec_16_1_*`, `opening_from_a_middle_panel_never_replaces_the_existing_child` (domain); UI test steps 1–6 |
| 3.2, 3.3, 12.2 | Separate branches; shared ancestors exist once | ✅ Verified | `spec_16_1_shared_ancestor_exists_once`; branch paths are derived from parent links |
| 3.5 | Switching restores the active panel, cursor, scroll, sort, view, filter and selection | ✅ Verified | `spec_16_3_panel_state_is_restored_when_returning`; UI test step 7 |
| 3.6 | Keyboard and mouse branch switching, active-branch indicator, overview | ✅ Verified | `[`/`]`, `:branch N`, the branch bar (active one underlined), `:branch` overview; UI test |
| 3.7 | Closing a branch keeps shared ancestors, never deletes files, and leaves running operations alone | ✅ Verified | `spec_16_1_closing_a_branch_keeps_shared_ancestors`, `spec_16_6_workspace_changes_do_not_alter_a_running_operation` |
| 3.8, 10.2, 10.5 | Branch visualisation: shared ancestors once, branches grouped, connectors, active branch highlighted, overview when space runs out | ✅ Verified | Side-by-side grid ([ADR 0005](../decisions/0005-branch-layout.md)): `workspace_layout` tests, including spans equal to branch counts; connector geometry and theme tests; UI test checks rows, columns, cards, connectors and the short-window fallback |
| 4.1 | `h`/`l` move within the branch and never create panels | ✅ Verified | `spec_16_2_horizontal_movement_stays_within_the_branch`; UI test |
| 4.2 | `j`/`k`, `gg`, `G`, `Ctrl+d`/`Ctrl+u`; configurable list ends | ✅ Verified | `cursor_movement_clamps_or_wraps` (wrapping is a preference); UI test |
| 4.3 | `Enter` opens; `Backspace` activates the parent panel | ✅ Verified | UI test; `parent_at_a_root_inserts_a_parent_panel` |
| 4.4 | Horizontal workspace scrolling, smooth, keeps the active panel visible | ✅ Verified | Animated reveal of the active panel; touchpad and wheel scrolling checked by hand on Hyprland and Niri ([2026-11-10](../development/verification-results/2026-11-10-hyprland-niri.md)) |
| 5.1 | Persistent panel state | ⚠️ Partial | Cursor, scroll, sort, view, filter, selection and hidden files are kept. Expanded metadata, column configuration and preview state are N/A: those features don't exist (D12) |
| 5.2 | Restoration and external changes | ✅ Verified | `spec_16_3_external_changes_keep_valid_state_and_drop_stale_selections` |
| 5.3 | State independent of widget lifetime | ✅ Verified | Views are dropped and recreated from `PanelState`; `evicted_panels_keep_their_state_and_reload_once` |
| 6 | Focus Mode: presentation only, navigation and branch switching keep working, preference for keeping it across branches | ✅ Verified | `spec_16_4_*` (domain and application); UI test steps 9–12. The transition itself is not animated |
| 6.6, 9 | Mouse: Focus Mode toggle, panel and branch activation, neighbour hints | ✅ Verified | Toolbar toggle, prev/next arrows in Focus Mode, branch bar, click to activate; checked by hand on Hyprland and Niri ([2026-11-10](../development/verification-results/2026-11-10-hyprland-niri.md)) |
| 7.1 | Normal Mode with configurable bindings | ✅ Verified | `KeyMap` tests (overrides, conflicts, prefixes, classic keys); `[keybindings]` in `settings.toml` |
| 7.2 | Command Mode: prompt, history, completion, errors, Esc | ✅ Verified | Command parser and `completion` tests (command names, folder paths for `:copy`/`:move`/`:open`, branch targets, smartcase, cycling); UI test step 14 and the completion steps |
| 7.3 | Visual Selection Mode with anchor and range | ✅ Verified | `spec_16_5_visual_mode_supports_range_selection`, `range_selection_covers_anchor_to_cursor` |
| 7.4 | Mode indicator; consistent Esc; no keys routed to the wrong mode | ✅ Verified | Mode bar; `spec_16_5_*`; key routing goes through one capture controller |
| 8.3 | Selection across panels and branches, with absolute sources | ✅ Verified | `spec_16_6_selection_spans_panels_and_branches`; UI test step 13 |
| 8.4 | Explicit destination: paste, command, drop, picker; clear summary | ✅ Verified | Paste into the active panel, `:copy <path | bN>`, destination picker, drop on a panel (dragging checked by hand on Hyprland and Niri, [2026-11-10](../development/verification-results/2026-11-10-hyprland-niri.md)); confirmation lists sources by folder (D7) |
| 8.5, 8.6 | The operation manager stays the single source of truth; D-Bus unchanged | ✅ Verified | Transfers are ordinary `OperationRequest`s; the IPC tests are unchanged and pass |
| 8.7 | Never overwrite; conflicts; selection kept on errors | ✅ Verified | `d15_same_named_sources_from_different_folders_never_overwrite`; the selection is cleared only after a successful submit |
| 11 | Search per panel | ✅ Verified | `search_filters_belong_to_their_panel`. Workspace-wide search is a future feature |
| 12 | Workspace as application state; navigation service | ✅ Verified | `WorkspaceService`; ADR 0004 |
| 13 | Versioned persistence; restore; missing paths | ✅ Verified | `TomlWorkspaceStore` tests; UI test restart. Missing folders show as unavailable panels |
| 14, 16.8 | Performance | ✅ Verified, with two open findings | Performance run of 2026-11-09 ([review](../review/performance.md)): all approved targets met at 50 branches and 211 panels after one fix (saving off the UI thread). Open: re-reads when cycling through more branches than the 16-listing cache covers (D13), and ~250 ms pauses when a 100,000-entry folder reloads live (exempt under R3) |
| 15 | Edge cases | ⚠️ Partial | Deleted folders and removed devices: panels kept and marked unavailable (D14). External renames are not followed. Closing the active branch in Focus Mode and resizing in Focus Mode are covered |

## Context-aware navigation specification

[2-CONTEXT-AWARE-NAVIGATION.md](../specifications/2-CONTEXT-AWARE-NAVIGATION.md), [ADR 0006](../decisions/0006-context-aware-navigation.md), decisions C1–C7.

| § | Requirement | Status | Evidence / notes |
|---|---|---|---|
| 3 | `l` opens the selected folder; `Enter` is exactly the same; files keep opening | ✅ Verified | Keymap and mode tests (`l` ≡ `<CR>` → `Intent::Open`); workspace UI test "Enter does exactly what l does". The plain move to the next panel is on `L` (C2) |
| 4.1 | Continue the active branch | ✅ Verified | `nav_16_2_1_*` (domain); UI test step 8 (`k`, `l`, `l`) |
| 4.2, 6 | Reactivate an existing branch with its downstream panels and state | ✅ Verified | `nav_16_2_3_*`, `nav_c1_*`, `nav_16_3_reactivation_restores_panel_state`, `nav_6_reactivation_reads_and_watches_nothing_new`; UI test "l on an existing child reactivates its branch" |
| 4.3 | Create a new branch from the current panel | ✅ Verified | `nav_16_2_2_*`; UI test step 5 |
| 5 | Directory identity separate from branch identity | ✅ Verified | Device and inode per panel (`reports_device_and_inode_*`, `nav_c3_*`); branch IDs unchanged and persisted |
| 7 | `[` and `]` unchanged, deterministic order | ✅ Verified | Existing branch tests and UI steps 7–11 |
| 8 | `h` returns to the parent without removing descendants | ✅ Verified | Existing tests; UI test |
| 9 | Focus Mode keeps working | ✅ Verified | Focus Mode tests unchanged; navigation never changes the presentation flag |
| 10 | Interaction modes | ✅ Verified | Command Mode never routes keys to navigation; in Visual Mode, `l` navigates folders but never launches files (C6) |
| 11 | Mouse uses the same logic | ✅ Verified | Double-click goes through `WorkspaceService::open_entry`, the single entry point |
| 12 | Persistence | ✅ Verified | Format 2 with identities; version 1 files restore unchanged (`version_1_files_restore_with_identities_unknown`) |
| 13 | Edge cases | ✅ Verified | Repeated opens never duplicate (`nav_16_2_5_*`); rename followed and replacement reset (C3); empty folder: `l` moves to the next panel (C2); links keep their own identity. A folder moved into a different parent is found by path only |

## Backward branch pruning specification

[3-BACKWARD-BRANCH-PRUNING.md](../specifications/3-BACKWARD-BRANCH-PRUNING.md), [ADR 0007](../decisions/0007-backward-branch-pruning.md), decisions B1–B9.

| § | Requirement | Status | Evidence / notes |
|---|---|---|---|
| 3, 13 | `d` operator: waits for a motion, `Esc` cancels, invalid motions are safe, pending keys shown | ✅ Verified | `prune_fires_only_after_the_complete_sequence`, `prune_incomplete_or_invalid_sequences_do_nothing` (`d<Esc>`, `dd`, `dj`, `d<Del>`), `prune_a_cancelled_operator_cannot_be_completed_later`; pending keys appear in the existing status bar indicator; UI test |
| 3 | Configurable bindings | ✅ Verified | `prune-back`, `prune-back-segment`, `undo-workspace` in `[keybindings]`; user bindings win over these new defaults (B8): `prune_user_bindings_win_over_the_new_defaults` |
| 4 | `db`: one navigation segment, destination kept with its state and made active | ✅ Verified | `prune_db_removes_one_navigation_step`, `prune_keeps_the_destination_state`; UI test. Segment = one panel (B1), so the spec's illustrative example (two panels removed) does not apply |
| 5 | `dB`: a larger segment, not a fixed count | ✅ Verified | Back to the nearest shared ancestor or the root (B1): `prune_db_b_*`; UI test. On a path without forks this differs from the spec's illustration |
| 6 | Deterministic segments from navigation relationships | ✅ Verified | Derived from the panel tree only; no model change was needed |
| 7, 9 | Shared ancestors and other branches untouched; stable identity; no dangling relationships | ✅ Verified | `prune_spec_7_*`, randomised invariant run with prune and undo (`invariants_hold_across_random_sequences_with_pruning_and_undo`); only the active branch's tip can be pruned (B2) |
| 8 | Active panel, selection, scroll, sort, view and filter kept; safe no-op with feedback | ✅ Verified | `prune_keeps_the_destination_state`, `prune_refuses_*`, `prune_refusals_change_nothing`; refusals are explained in the status bar. Expanded metadata doesn't exist (D12) |
| 10 | Workspace undo | ✅ Verified | `u`, re-attaching the removed panels (B5): `prune_undo_*` (domain and application); UI test. Session only, 20 changes. Focus Mode isn't part of what undo changes, because pruning never changes it (B4) |
| 11 | Pruning in Focus Mode | ⛔ Changed by decision | B9 (2026-11-11): `db`, `dB` and `u` are turned off in Focus Mode and say so; Focus Mode and the workspace are left as they were. `prune_and_undo_are_refused_in_focus_mode`; UI test |
| 12 | Command Mode untouched; Visual Mode not disturbed; never file deletion | ✅ Verified | `prune_keys_go_to_the_command_line_in_command_mode`, `prune_is_not_available_in_visual_mode_and_never_reaches_the_toolkit` (B6) |
| 14 | Clean Architecture | ✅ Verified | Domain `workspace/prune.rs`, application `prune_back`/`undo_workspace`, presentation only reports; architecture rules pass |
| 15, 16.7 | Files never modified | ✅ Verified | `prune_db_only_unwatches_removed_panels_and_saves` (effects are `Unwatch` and `Persist` only), `prune_never_changes_files_or_running_operations`; the UI test compares a fingerprint of the home folder before and after |
| 15 | Regressions: `l`, `Enter`, `h`, `[`/`]`, Visual, Command, persistence, Focus Mode | ✅ Verified | `prune_keeps_l_h_and_branch_switching_working`, `prune_results_persist_and_restore`; every earlier test and UI check passes unchanged |

## Known gaps and follow-ups

1. **Performance follow-ups** from the [2026-11-09 run](../review/performance.md): the listing cache size (D13) for workspaces with many branches, and incremental model updates for live reloads of very large folders. Deferred (2026-11-09, R5): revisit only if needed.
2. **Manual verification on GNOME and KDE Plasma, and with Orca.** The full checklist passed on Hyprland and Niri ([2026-11-10](../development/verification-results/2026-11-10-hyprland-niri.md)), including drag and drop, scaling, touchpad scrolling, devices and the Trash.
3. **D12 panel features** (columns, expanded metadata, preview), if they are wanted later.
