# Feature Specification: Context-Aware Spatial Navigation

**Status:** Proposed  
**Priority:** Core navigation  
**Scope:** Navigation behaviour, branch resolution and workspace integration  
**Platform:** Linux-first  
**Architecture:** Existing Clean Architecture

---

# 1. Overview

Update the spatial file manager's navigation system to provide more natural, Vim-inspired directory navigation.

The primary change is to make `l` the directory-opening key, with context-aware behaviour that determines whether the user is continuing an existing branch, returning to an existing branch or exploring a new branch.

The `Enter` key must perform exactly the same action as `l`.

The navigation system must intelligently reuse existing workspace state rather than creating duplicate branches or panels unnecessarily.

The existing spatial workspace model remains authoritative. This change must extend the current implementation rather than introduce a separate navigation model.

## 1.1 Core principles

1. Navigation should feel natural and predictable.
2. Existing branches should be reused wherever possible.
3. New branches should be created only when necessary.
4. Existing workspace state must never be discarded implicitly.
5. Shared ancestors must remain logically shared.
6. Branch navigation must remain independent of panel navigation.
7. Keyboard interaction must be consistent with the existing Vim-inspired model.
8. Focus Mode must remain a presentation state, not a separate navigation system.

# 2. Navigation model

The workspace consists of persistent branches containing connected directory panels.

A branch represents a navigable directory path and maintains its own navigation history.

Multiple branches may share ancestors.

Example:

Home
├── Documents
│   └── Projects
│       └── Rust
└── Downloads
    └── Archives

In this example:

- Home is a shared ancestor.
- Documents, Projects and Rust form one branch.
- Downloads and Archives form another branch.
- Each branch maintains independent navigation state.
- Switching branches restores the previously active panel and its state.

The same directory must not be duplicated unnecessarily simply because it is reachable from multiple branches.

Directory identity and branch identity are separate concepts.

A directory represents a filesystem location. A branch represents a particular navigation journey through the workspace.

# 3. Keyboard navigation

## 3.1 Default key bindings

The proposed default navigation bindings are:

| Key | Action |
|---|---|
| `h` | Navigate to parent panel |
| `j` | Move selection down |
| `k` | Move selection up |
| `l` | Open selected directory |
| `Enter` | Same action as `l` |
| `[` | Switch to previous branch |
| `]` | Switch to next branch |
| `Esc` | Existing mode-dependent escape behaviour |

These bindings must be checked against the existing keyboard specification and made configurable where the application already supports configurable shortcuts.

Do not silently change unrelated shortcuts.

## 3.2 The `l` key

When the user presses `l` on a selected directory:

1. Resolve the selected directory's filesystem identity.
2. Determine the current panel and active branch.
3. Inspect the workspace for an appropriate existing destination panel or branch.
4. Determine whether the action continues the current branch, reactivates an existing branch or creates a new branch.
5. Perform the resolved navigation action.
6. Move the active panel to the resulting directory.
7. Preserve all unrelated workspace state.

The resolution must happen through an application-layer navigation use case.

The UI must not independently decide whether to create or reactivate branches.

## 3.3 The `Enter` key

`Enter` must be an exact alias for `l` when a directory is selected.

Both keys must invoke the same application-layer operation.

There must not be separate implementations or subtly different navigation semantics.

## 3.4 Behaviour on files

When the selected item is a regular file:

- `l` and `Enter` must retain the established file-opening behaviour.
- They must not attempt directory navigation.
- File opening must use the existing file-opening mechanism.
- Unsupported or unavailable file handlers must produce an appropriate error.

Other file types, including symbolic links and special filesystem entries, must follow the application's established file-handling rules.

# 4. Context-aware branch resolution

This is the central feature of the change.

When opening a directory, the navigation system must determine which of three actions is appropriate:

1. Continue the active branch.
2. Reactivate an existing branch.
3. Create a new branch.

The resolution order is important.

## 4.1 Priority 1: Continue the active branch

If the selected directory is the natural continuation of the active branch:

- Create or activate the next panel to the right.
- Navigate into the selected directory.
- Move the active panel to the new directory.
- Preserve the existing parent panel.
- Preserve all other branches.

Example:

Before:

Home → Documents

The user selects Projects and presses `l`.

After:

Home → Documents → Projects

This is a continuation of the active branch, not a new branch.

If the destination panel already exists as the next panel in the active branch, reactivate it rather than creating another panel.

If the branch has an existing continuation beyond the destination, preserve that continuation and restore its existing state.

## 4.2 Priority 2: Reactivate an existing branch

If the selected directory corresponds to an existing branch or an existing branch continuation, reactivate that existing workspace path.

Do not create duplicate branches.

Example:

Existing workspace:

Home
├── Documents
│   └── Projects
└── Downloads
    └── Archives

The user navigates back to Home.

They select Documents and press `l`.

Expected result:

- Reactivate the existing Documents branch.
- Restore Projects as its existing continuation.
- Restore the branch's previous active panel.
- Restore selection and scroll positions.
- Preserve Downloads and Archives.
- Do not create duplicate Documents or Projects panels.

This must work even if the existing branch is not currently active.

Reactivation should restore the branch's previous state rather than simply opening a fresh panel at the selected directory.

## 4.3 Priority 3: Create a new branch

If the selected directory neither continues the active branch nor corresponds to an appropriate existing branch:

- Create a new independent branch.
- Use the current directory context as the branch's starting point.
- Reuse shared ancestors.
- Create the destination panel.
- Activate the new branch.
- Move focus to the destination panel.
- Preserve every existing branch.

Example:

Existing workspace:

Home
└── Documents
    └── Projects

The user returns to Home, selects Downloads and presses `l`.

Expected result:

Home
├── Documents
│   └── Projects
└── Downloads

The Documents branch remains intact.

Downloads becomes a new independent branch.

The new branch can subsequently be navigated without changing the Documents branch's history.

# 5. Branch identity and directory identity

The implementation must explicitly distinguish between a filesystem location and a workspace navigation path.

## 5.1 Directory identity

A directory should be identified using a stable filesystem identity where available.

Do not rely exclusively on:
- Display name.
- Filename.
- Relative path.
- Visible panel position.

Filesystem identity must account for platform-specific path semantics.

The implementation must also handle cases where filesystem identity cannot be obtained reliably.

## 5.2 Branch identity

Each branch must retain a stable identifier independent of the directory it currently displays.

Branch identity must survive:
- Switching branches.
- Returning to ancestors.
- Opening existing directories.
- Reactivating branches.
- Workspace persistence and restoration.

Two branches must not accidentally become one simply because they share an ancestor.

## 5.3 Shared ancestors

Shared ancestors must be represented logically rather than duplicated as independent underlying workspace objects.

For example:

Home
├── Documents
└── Downloads

Both branches refer to the same logical Home directory.

However, each branch retains its own navigation continuation and history.

The implementation must avoid confusing shared directory identity with branch ownership.

# 6. Existing branch reactivation

Reactivation is a first-class navigation action.

When an existing branch is selected through `l`:

1. Resolve the target branch.
2. Retrieve its stored navigation state.
3. Activate the branch.
4. Restore its previously active panel.
5. Restore panel state.
6. Update the workspace's active branch reference.
7. Update the visible spatial layout.
8. Preserve all other branches.

The following state must be restored wherever applicable:

- Active panel.
- Selected item.
- Vertical scroll position.
- Sort order.
- View mode.
- Search filter.
- Expanded metadata.
- Panel history.
- Existing downstream panels.

Reactivation must not:
- Reload unrelated branches.
- Duplicate panels.
- Reset panel history.
- Destroy the branch being left.
- Create unnecessary filesystem watchers.
- Trigger duplicate file enumeration without a reason.

If the filesystem has changed since the branch was last active, update the relevant panel contents while preserving as much user state as possible.

If the previously selected item no longer exists, choose a sensible fallback without resetting the entire panel.

# 7. Branch switching

The existing `[` and `]` controls must remain the explicit branch-switching mechanism.

## 7.1 Previous branch

`[` activates the previous branch in the workspace.

## 7.2 Next branch

`]` activates the next branch in the workspace.

## 7.3 Branch switching behaviour

When switching branches:

- Restore the target branch's previous active panel.
- Restore its panel state.
- Preserve the branch being left.
- Do not create or destroy branches.
- Preserve Focus Mode state according to the existing Focus Mode specification.

Branch order must be deterministic and must not change unpredictably as a side effect of directory navigation.

The exact definition of branch order should follow the existing workspace implementation. If no deterministic order exists, define one before implementation and document it.

# 8. Parent navigation with `h`

When pressing `h`:

- Navigate to the parent panel within the active branch.
- Restore the parent's previous state.
- Move active focus to the parent panel.
- Preserve the child panel and its state.
- Do not destroy the branch continuation.

If the current panel is already the branch's root or has no parent panel in the workspace, follow the existing root-navigation behaviour.

Returning to an ancestor must not automatically remove descendants.

This is essential because the user may return to an ancestor and explore another directory while preserving the original branch.

# 9. Focus Mode integration

Focus Mode expands the active panel to fill the available workspace while hiding neighbouring panels from the main view.

It is a presentation state, not a navigation mode.

The new navigation behaviour must work identically in Focus Mode and normal multi-panel mode.

When `l` opens or reactivates a directory:

- Keep Focus Mode enabled.
- Move focus to the resulting panel.
- Hide neighbouring panels from the main view.
- Preserve the underlying workspace layout and state.

When using `h`, `[` or `]` in Focus Mode:

- Navigation must continue to work.
- Focus Mode must remain enabled.
- The active panel must change appropriately.
- The hidden workspace must remain intact.

Exiting Focus Mode must restore the multi-panel workspace without duplicate panels or lost state.

# 10. Interaction mode integration

The application currently defines:

- Normal Mode.
- Command Mode.
- Visual Selection Mode.

The new navigation behaviour must respect the established responsibilities of each mode.

## 10.1 Normal Mode

The new `h`, `j`, `k`, `l`, `[` and `]` behaviour applies as specified.

## 10.2 Command Mode

Do not allow navigation keys to interfere with command input.

Existing command-mode semantics take precedence.

## 10.3 Visual Selection Mode

Preserve existing selection semantics.

Do not accidentally open directories or switch branches when the same key is being used for selection-related behaviour.

Review the current Visual Selection specification and define any necessary overrides explicitly.

# 11. Mouse interaction

The keyboard model must not make mouse interaction less capable.

Mouse interaction should provide equivalent navigation operations:

- Double-clicking a directory opens it using the same context-aware branch-resolution logic.
- Clicking a branch selector activates that branch.
- Clicking a panel activates that panel.
- Existing context-menu actions remain available.

Mouse and keyboard navigation must call the same underlying application-layer use cases.

Do not implement separate branch-resolution logic for mouse interaction.

# 12. Workspace persistence

The existing workspace persistence model must continue to work.

Persist enough information to restore:

- Branch identities.
- Branch ordering.
- Panel identities.
- Panel relationships.
- Shared ancestors.
- Active branch.
- Active panel.
- Per-panel state.
- Navigation history.
- Focus Mode state, if already persisted by the existing implementation.

On application restart, restore the workspace consistently.

The implementation must not introduce duplicate branches during restoration.

If a persisted directory is unavailable, restore the workspace gracefully and report the unavailable location without losing unrelated branches.

# 13. Edge cases

The following cases must be considered explicitly.

## 13.1 Directory already open in the active branch

Reactivate the existing panel or continuation rather than creating a duplicate.

## 13.2 Directory already open in another branch

Reactivate the appropriate existing branch if the selected directory corresponds to that branch's entry point or an existing continuation.

Do not create a duplicate branch unnecessarily.

## 13.3 Directory is a shared ancestor

Preserve its shared identity.

Do not create duplicate logical ancestor panels merely because the user enters it from another branch.

## 13.4 Same directory reached through different paths

Use filesystem identity and workspace relationships carefully.

Do not assume two identical directory names represent the same directory.

If aliases, symbolic links or mount points refer to the same underlying location, follow the application's established path-identity policy.

Do not collapse genuinely distinct navigation contexts without an explicit rule.

## 13.5 Directory renamed or moved

If a directory changes location, handle stale branch references gracefully.

Do not silently navigate to a different directory that happens to have the same name.

## 13.6 Directory deleted or inaccessible

Report the error and preserve unrelated workspace state.

Do not create an empty or misleading branch.

## 13.7 Empty directory

Opening an empty directory must still create or activate the appropriate panel.

## 13.8 Symbolic links

Follow existing filesystem navigation policy.

Avoid infinite traversal and identity confusion.

## 13.9 Rapid navigation

Rapid repeated key presses must not create duplicate panels or race against unfinished branch-resolution operations.

Navigation operations must be serialized or otherwise coordinated safely.

# 14. Architecture

This change must follow the project's existing Clean Architecture.

Do not move workspace logic into UI components.

## 14.1 Domain layer

The domain layer should own the concepts and invariants relating to:

- Branch identity.
- Panel identity.
- Panel relationships.
- Shared directory ancestry.
- Navigation history.
- Workspace state.

It should not depend on the UI framework.

## 14.2 Application layer

Introduce or extend a navigation use case that resolves directory-opening intent.

Conceptually, the resolution should produce one of:

- ContinueActiveBranch.
- ReactivateExistingBranch.
- CreateNewBranch.

The exact types and API should fit the existing codebase.

The application layer must:
- Resolve the intended navigation action.
- Coordinate workspace state changes.
- Preserve branch invariants.
- Handle navigation failures.
- Publish appropriate workspace events.

## 14.3 Infrastructure layer

Infrastructure is responsible for:
- Filesystem identity.
- Path resolution.
- Filesystem metadata.
- Persistence.
- Filesystem change detection.

Do not make the domain depend directly on platform-specific filesystem APIs.

## 14.4 Presentation layer

Presentation is responsible for:
- Keyboard event handling.
- Mouse event handling.
- Active panel indication.
- Branch indicators.
- Panel transitions.
- Rendering the workspace.

The presentation layer must dispatch navigation requests rather than implementing branch detection.

## 14.5 Event handling

Workspace changes should use the existing application event or state-update mechanisms.

Avoid duplicated notifications or unnecessary reloads.

Any new events must integrate with the existing architecture.

# 15. Suggested application-level API

This is illustrative rather than a requirement to use these exact names.

A possible use case:

`OpenSelectedDirectory`

Inputs:
- Active branch ID.
- Active panel ID.
- Selected directory identity.

Output:
- Navigation resolution.
- Resulting active branch ID.
- Resulting active panel ID.

Possible resolution types:

- `ContinueActiveBranch`
- `ReactivateExistingBranch`
- `CreateNewBranch`

The implementation should make the resolution explicit and testable.

Do not expose UI-specific state through the domain API.

# 16. Testing requirements

Testing is critical because branch resolution is central to the product's identity.

## 16.1 Unit tests

Test:
- Directory identity resolution.
- Branch identity.
- Branch continuation detection.
- Existing branch detection.
- Shared ancestor relationships.
- Navigation resolution priority.
- Stable panel identities.
- Navigation history restoration.

## 16.2 Navigation behaviour tests

Test:

1. Opening a child directory continues the active branch.
2. Returning to an ancestor and opening a different child creates a new branch.
3. Returning to an ancestor and reopening an existing child reactivates its branch.
4. Reopening an existing branch restores its downstream panels.
5. Reopening an existing panel does not create duplicates.
6. Opening a directory already represented elsewhere behaves predictably.
7. Shared ancestors remain logically shared.
8. `Enter` and `l` produce identical results.
9. `h` returns to the parent without destroying descendants.
10. `[` and `]` restore the correct branches.

## 16.3 State preservation tests

Verify:
- Selection is preserved.
- Scroll position is preserved.
- Sorting is preserved.
- View mode is preserved.
- Search filters are preserved.
- Expanded metadata is preserved.
- Panel history is preserved.
- Unrelated branches are unchanged.

## 16.4 Focus Mode tests

Verify:
- Opening a directory retains Focus Mode.
- Reactivating a branch retains Focus Mode.
- Parent navigation works in Focus Mode.
- Branch switching works in Focus Mode.
- Exiting Focus Mode restores the original workspace layout.

## 16.5 Regression tests

Ensure that:
- Existing spatial navigation continues to work.
- Existing Visual Selection behaviour is unchanged.
- Command Mode remains functional.
- File opening remains functional.
- Workspace persistence remains compatible.
- Search and file operations continue to work.
- Existing D-Bus operation reporting is unaffected.

# 17. Acceptance criteria

The change is complete when:

1. `l` opens the selected directory.
2. `Enter` performs exactly the same operation.
3. Continuing the active branch extends that branch.
4. Opening a different directory from an ancestor creates a new branch.
5. Reopening an existing branch reactivates it rather than duplicating it.
6. Existing branch descendants and panel state are restored.
7. Shared ancestors remain logically shared.
8. Unrelated branches are never implicitly destroyed.
9. `h` navigates to the parent without destroying descendants.
10. `[` and `]` continue to switch between branches.
11. Focus Mode remains functional.
12. All interaction modes respect their existing semantics.
13. Keyboard and mouse navigation use the same underlying logic.
14. Navigation remains reliable under rapid input.
15. Automated tests cover branch resolution and state restoration.
16. Existing architectural and platform requirements remain satisfied.

# 18. Implementation instructions for Claude Code

Before implementation:

1. Read the root `CLAUDE.md`.
2. Read the original product specification.
3. Read the spatial workspace specification.
4. Read the existing navigation and keyboard specifications.
5. Inspect the current spatial workflow proof of concept.
6. Inspect the actual workspace, branch and panel data models.
7. Identify how navigation history is currently stored.
8. Identify how shared ancestors are represented.
9. Identify how workspace persistence works.
10. Identify any conflicts with existing navigation behaviour.

Then produce:

- A summary of the current implementation.
- A mapping of the new requirements onto existing architecture.
- A clear explanation of the branch-resolution algorithm.
- Any ambiguities or edge cases requiring decisions.
- A phased implementation plan.
- A test plan.
- Any required data-model or persistence changes.

Do not begin implementation until the proposed design has been reviewed and approved.

Do not replace the existing workspace model.

Do not simplify branching into ordinary tabs or a single linear navigation history.

Do not duplicate existing navigation services.

Do not remove or weaken existing architectural, Wayland, D-Bus or file-operation requirements.

Implement incrementally and update the relevant documentation alongside the code.

# 19. Definition of done

Directory navigation is context-aware and predictable.

Users can explore multiple directory paths without losing their previous navigation journeys.

Existing paths are automatically reactivated when revisited.

New branches are created only when genuinely needed.

The result feels like a coherent spatial navigation system rather than a conventional file manager with additional panels.
