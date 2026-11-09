# Implementation status

Status as of 2026-11-09, branch `worktree-spatial-workspace`.

"Verified" means checked by an automated test or script in this repository, unless marked *manual*. The original specification's status is kept in [definition-of-done.md](../definition-of-done.md), and it still holds: every original check passes unchanged, except the UI test now starts searches with `/` (decision D1).

**Checks:** `scripts/check.sh --ui` passes. That covers fmt, clippy with `-D warnings`, the architecture rules, 273 Rust tests (up from 165), native Wayland verification, and both sandboxed UI tests: file operations (20 checks) and the spatial workspace (37 checks).

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
| 4.4 | Horizontal workspace scrolling, smooth, keeps the active panel visible | ✅ / *manual* | Animated reveal of the active panel; touchpad and wheel scrolling come from GTK (*manual*) |
| 5.1 | Persistent panel state | ⚠️ Partial | Cursor, scroll, sort, view, filter, selection and hidden files are kept. Expanded metadata, column configuration and preview state are N/A: those features don't exist (D12) |
| 5.2 | Restoration and external changes | ✅ Verified | `spec_16_3_external_changes_keep_valid_state_and_drop_stale_selections` |
| 5.3 | State independent of widget lifetime | ✅ Verified | Views are dropped and recreated from `PanelState`; `evicted_panels_keep_their_state_and_reload_once` |
| 6 | Focus Mode: presentation only, navigation and branch switching keep working, preference for keeping it across branches | ✅ Verified | `spec_16_4_*` (domain and application); UI test steps 9–12. The transition itself is not animated |
| 6.6, 9 | Mouse: Focus Mode toggle, panel and branch activation, neighbour hints | ✅ / *manual* | Toolbar toggle, prev/next arrows in Focus Mode, branch bar, click to activate |
| 7.1 | Normal Mode with configurable bindings | ✅ Verified | `KeyMap` tests (overrides, conflicts, prefixes, classic keys); `[keybindings]` in `settings.toml` |
| 7.2 | Command Mode: prompt, history, completion, errors, Esc | ✅ Verified | Command parser and `completion` tests (command names, folder paths for `:copy`/`:move`/`:open`, branch targets, smartcase, cycling); UI test step 14 and the completion steps |
| 7.3 | Visual Selection Mode with anchor and range | ✅ Verified | `spec_16_5_visual_mode_supports_range_selection`, `range_selection_covers_anchor_to_cursor` |
| 7.4 | Mode indicator; consistent Esc; no keys routed to the wrong mode | ✅ Verified | Mode bar; `spec_16_5_*`; key routing goes through one capture controller |
| 8.3 | Selection across panels and branches, with absolute sources | ✅ Verified | `spec_16_6_selection_spans_panels_and_branches`; UI test step 13 |
| 8.4 | Explicit destination: paste, command, drop, picker; clear summary | ✅ / *manual* | Paste into the active panel, `:copy <path | bN>`, destination picker, drop on a panel (*manual*: wtype can't drag); confirmation lists sources by folder (D7) |
| 8.5, 8.6 | The operation manager stays the single source of truth; D-Bus unchanged | ✅ Verified | Transfers are ordinary `OperationRequest`s; the IPC tests are unchanged and pass |
| 8.7 | Never overwrite; conflicts; selection kept on errors | ✅ Verified | `d15_same_named_sources_from_different_folders_never_overwrite`; the selection is cleared only after a successful submit |
| 11 | Search per panel | ✅ Verified | `search_filters_belong_to_their_panel`. Workspace-wide search is a future feature |
| 12 | Workspace as application state; navigation service | ✅ Verified | `WorkspaceService`; ADR 0004 |
| 13 | Versioned persistence; restore; missing paths | ✅ Verified | `TomlWorkspaceStore` tests; UI test restart. Missing folders show as unavailable panels |
| 14, 16.8 | Performance | ✅ Verified, with two open findings | Performance run of 2026-11-09 ([review](../review/performance.md)): all approved targets met at 50 branches and 211 panels after one fix (saving off the UI thread). Open: re-reads when cycling through more branches than the 16-listing cache covers (D13), and ~250 ms pauses when a 100,000-entry folder reloads live (exempt under R3) |
| 15 | Edge cases | ⚠️ Partial | Deleted folders and removed devices: panels kept and marked unavailable (D14). External renames are not followed. Closing the active branch in Focus Mode and resizing in Focus Mode are covered |

## Known gaps and follow-ups

1. **Drag and drop between panels** is wired through the existing drop targets with explicit destinations, but needs a *manual* check: the UI harness cannot drag.
2. **Performance follow-ups** from the [2026-11-09 run](../review/performance.md): the listing cache size (D13) for workspaces with many branches, and incremental model updates for live reloads of very large folders. Deferred (2026-11-09, R5): revisit only if needed.
3. **Manual Wayland matrix** (GNOME, KDE, fractional scaling, touchpad horizontal scrolling, connectors at fractional scales). Pending, as before.
4. **D12 panel features** (columns, expanded metadata, preview), if they are wanted later.
