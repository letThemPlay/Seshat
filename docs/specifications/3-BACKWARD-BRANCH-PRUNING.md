# Feature Specification: Vim-Inspired Backward Branch Pruning

## 1. Overview

Introduce Vim-inspired backward pruning operations for the spatial file manager.

The feature allows users to remove panels from the active branch by using familiar Vim-style operator and motion combinations.

Initial supported commands:

- `db`
- `dB`

These commands operate on workspace navigation structure only.

They must never delete, move, rename or otherwise modify filesystem contents.

The feature must integrate with the existing branching workspace, panel history, keyboard interaction modes and Clean Architecture.

## 2. Core terminology

### Panel

A directory view within the spatial workspace.

### Branch

An independently navigable directory path with its own continuation and navigation history.

### Navigation segment

A logical grouping of panels used to determine backward pruning distance.

Navigation segments must be derived from workspace navigation relationships, not simply directory-name characters.

### Pruning

Removing panels and their navigation relationships from the workspace while preserving filesystem contents.

## 3. Keyboard interaction

Use `d` as a pending operator in Normal Mode.

The operator must not execute an action until a valid motion is received.

Initial supported combinations:

| Input | Action |
|---|---|
| `db` | Backward prune by one navigation segment |
| `dB` | Backward prune by one larger navigation segment |
| `Esc` | Cancel pending prune operator |

Do not implement additional Vim motions or counts in this initial phase.

The key bindings must respect the existing configurable shortcut system where available.

## 4. `db`: backward pruning

`db` removes the previous navigation segment from the active branch.

The operation:
- Starts from the active panel.
- Resolves the previous navigation segment.
- Removes the panels between the resolved destination and the active panel.
- Retains the destination panel.
- Makes the destination panel active.
- Preserves its existing state.
- Preserves unrelated branches.

The operation must be deterministic and must not depend on directory names.

Example:

Before:

Home → Documents → Projects → Rust → Cargo

Active panel: Cargo

After `db`:

Home → Documents → Projects

Active panel: Projects

This example illustrates the intended behaviour; actual segment boundaries must follow the workspace's documented navigation model.

## 5. `dB`: backward pruning by a larger segment

`dB` performs the equivalent operation using a larger navigation segment.

It should travel further backwards than `db`.

Example:

Before:

Home → Documents → Projects → Rust → Cargo

Active panel: Cargo

After `dB`:

Home → Documents

Active panel: Documents

The precise larger-segment boundary must be defined using workspace navigation relationships.

Do not implement this as an arbitrary fixed number of panels.

## 6. Navigation segment resolution

This is an important architectural responsibility.

Vim distinguishes between `b` and `B` using different word-boundary rules.

The file manager should provide an analogous distinction using spatial navigation concepts.

Suggested initial interpretation:

- `db`: Move backwards across one logical navigation step or segment.
- `dB`: Move backwards across one larger branch-history segment.

The implementation must inspect the existing workspace model and propose a deterministic definition of these segments.

Do not use directory-name tokenisation as the primary mechanism.

Do not infer segment boundaries from visual panel positions alone.

If the current workspace model does not contain enough information to distinguish these units reliably, propose the smallest necessary model enhancement before implementation.

## 7. Branch safety and shared ancestors

Pruning must respect the branching workspace model.

Example:

Home
├── Documents
│   └── Projects
│       └── Rust
└── Downloads
    └── Archives

If pruning the Documents branch:

- Do not remove Home if Downloads still references it.
- Do not modify Downloads or Archives.
- Preserve all unrelated branch state.
- Remove only the appropriate Documents branch continuation.

Shared ancestors must remain logically shared.

Branch identity must remain stable.

A pruning operation must never leave dangling panel or branch relationships.

## 8. Active panel behaviour

After pruning:

- The destination panel becomes active.
- Its selection is restored.
- Its scroll position is restored.
- Its sort order and view mode are preserved.
- Its search filter and expanded metadata are preserved.
- The removed downstream panels are no longer part of the active branch continuation.

If no valid backward destination exists, the operation must safely do nothing and provide appropriate feedback.

## 9. Other branches

Backward pruning must be scoped to the active branch.

It must never prune panels from another independent branch merely because they share ancestors or appear nearby in the workspace layout.

If a panel is shared by other branches, preserve its shared identity and all relationships still required by those branches.

## 10. Workspace undo

Pruning should be reversible.

Introduce or extend workspace-level undo support.

Undo must restore:
- Removed panels.
- Branch relationships.
- Panel ordering.
- Active branch.
- Active panel.
- Panel selection.
- Scroll positions.
- Sort and view settings.
- Search filters.
- Expanded metadata.
- Focus Mode state.

Undo is a workspace operation and must not be confused with undoing filesystem changes.

If the project already has a workspace undo mechanism, extend it rather than creating a competing system.

If no such mechanism exists, assess the implementation cost and propose an appropriate minimal design before proceeding.

## 11. Focus Mode

Both `db` and `dB` must work in Focus Mode.

Requirements:
- Keep Focus Mode enabled after pruning.
- Focus the resulting destination panel.
- Preserve hidden workspace state.
- Exiting Focus Mode must display the correctly pruned workspace.
- Do not duplicate or reload unrelated panels.

## 12. Interaction modes

### Normal Mode

`db` and `dB` are available as specified.

### Command Mode

Do not intercept text input.

### Visual Selection Mode

Do not allow pruning keys to interfere with selection behaviour unless explicitly defined by the existing mode specification.

The `d` operator must not accidentally trigger file deletion or file-operation behaviour.

## 13. Pending operator behaviour

When the user presses `d`:

- Enter a pending prune-operator state.
- Wait for a supported motion.
- Provide subtle visual feedback that an operator is pending.
- Cancel the pending operation on `Esc`.
- Handle invalid motions safely.
- Do not modify workspace state until a complete valid command has been received.

Avoid accidental execution from partial or invalid key sequences.

## 14. Architecture

Follow Clean Architecture.

### Domain

Own workspace pruning invariants and the concepts required to identify navigation segments.

### Application

Implement an application-layer use case responsible for resolving and executing a backward prune operation.

The use case should receive:
- Active branch identity.
- Active panel identity.
- Prune motion (`b` or `B`).

It should resolve the destination and produce a workspace change.

### Infrastructure

Handle any persistence requirements for workspace state and undo.

### Presentation

Handle:
- Pending `d` operator state.
- Motion input.
- Visual feedback.
- Dispatching the prune request.
- Rendering the resulting workspace.

Do not implement branch traversal or pruning algorithms in UI components.

## 15. Testing

### Unit tests

Test:
- Navigation segment resolution.
- Backward destination selection.
- `b` versus `B` semantics.
- Branch boundary handling.
- Shared ancestor handling.
- Invalid pruning requests.
- Stable panel identity.

### Integration tests

Test:
1. `db` prunes the expected backward segment.
2. `dB` prunes a larger backward segment.
3. Active panel moves to the correct destination.
4. Destination panel state is preserved.
5. Unrelated branches remain unchanged.
6. Shared ancestors remain available.
7. Pruning does not modify filesystem contents.
8. Undo restores the original workspace.
9. Focus Mode remains enabled.
10. Invalid motions do not modify the workspace.
11. Pending `d` can be cancelled using `Esc`.

### Regression tests

Verify existing:
- Spatial navigation.
- Branch switching.
- Directory opening with `l` and `Enter`.
- Visual Selection.
- Command Mode.
- Workspace persistence.
- Focus Mode.

## 16. Acceptance criteria

The feature is complete when:

1. `db` performs backward pruning by one defined navigation segment.
2. `dB` performs backward pruning by a larger defined navigation segment.
3. The active destination panel is retained and activated.
4. Pruned panels are removed from the appropriate workspace continuation.
5. Other branches remain unaffected.
6. Shared ancestors are preserved.
7. Filesystem contents are never modified.
8. Invalid and incomplete commands are safe.
9. Workspace state can be restored through undo, if the approved design includes undo.
10. Focus Mode and existing interaction modes behave correctly.
11. Automated tests cover branch safety and pruning semantics.

## 17. Implementation instructions for Claude Code

Before implementation:

1. Read root CLAUDE.md.
2. Read the foundational product specification.
3. Read the spatial workspace specification.
4. Read the context-aware navigation specification.
5. Inspect the current workspace and navigation implementation.
6. Identify how navigation history and branch relationships are represented.
7. Determine how logical navigation segments can be identified.
8. Propose exact `b` and `B` boundary semantics.
9. Identify whether workspace undo already exists.
10. Provide an implementation and testing plan.

Do not implement until the semantics have been reviewed and approved.

Do not treat pruning as filesystem deletion.

Do not introduce a separate branch or panel management system.

Do not simplify the existing branching architecture to accommodate this feature.

## 18. Definition of done

Users can prune backward through their active spatial navigation history using familiar Vim-inspired commands.

Pruning changes the workspace structure only.

It is predictable, branch-safe, reversible where supported and fully integrated with the existing spatial navigation model.
