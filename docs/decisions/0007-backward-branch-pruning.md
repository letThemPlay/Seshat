# ADR 0007: Backward branch pruning and workspace undo

- **Status:** Accepted
- **Date:** 2026-11-11
- **Spec references:** [backward branch pruning](../specifications/3-BACKWARD-BRANCH-PRUNING.md); decisions B1–B9 in [DECISIONS.md](../architecture/DECISIONS.md)

## Context

The specification adds Vim-style `db` and `dB` to remove panels from the active branch, working backwards, with workspace undo. Pruning changes the workspace structure only and must never touch files. It must protect shared ancestors and other branches, and it has to fit the existing model of [ADR 0004](0004-spatial-workspace.md) and [ADR 0006](0006-context-aware-navigation.md) without simplifying it.

The model already had most of what was needed:
- Panels form a forest.
- A branch is a leaf panel (its tip) plus a remembered panel (its cursor).
- Paths are derived from parent links.
- Branches and leaves correspond one to one.
- `close_branch` already removes panels upwards until it reaches a panel other branches still use.

It lacked three things: a definition of "navigation segment", a prune operation and any undo.

**The model has no way to tell one panel from another on a path without forks.** The spec rules out directory names, positions on screen and fixed counts. So the spec's §4/§5 examples, which remove 2 and 3 panels from the single path `Home → Documents → Projects → Rust → Cargo`, cannot follow from any allowed rule. The spec says those examples are only illustrations.

## Decision

### Segments come from the tree's structure (B1)

- **`db`** (`PruneMotion::Step`) goes back one navigation step, which is one panel. The destination is the active panel's parent.
- **`dB`** (`PruneMotion::Segment`) goes back one branch segment. The destination is the nearest ancestor that has two or more children, which means another branch also passes through it. If there is no such ancestor, the destination is the branch's root.

The model needs no new data: forks and roots are derived from the tree.

### Only the end of the active branch can be pruned (B2)

The active panel must be the active branch's tip. The removed panels are the ones between the destination and the active panel, including the active panel.

By construction, every removed panel has exactly one child. So no other branch passes through them, and a shared ancestor can never be removed.

If panels continue beyond the active panel, the prune is refused (`PanelsBeyond`, or `Shared` when other branches pass through it). Those panels live inside the active panel's folder and could not be re-attached anywhere else. Removing them too would delete panels the motion never covered.

The destination keeps its full state and becomes active.

### Pruning back to a shared ancestor ends the branch (B3)

If the destination is shared, it still has children after the prune, so it is not a leaf. The active branch therefore has no panels of its own left, and it ends.

The branch through the destination that was most recently active takes over, with its cursor on the destination. Clicking a shared panel already works this way (D6).

Other branches keep their panels, ids, tips and state. Removed panels are dropped from the Back/Forward jump list.

### Focus Mode (B4, B9)

- `db`, `dB` and `u` are **refused in Focus Mode** (B9, a product decision of 2026-11-11). The refusal lives in `WorkspaceService`, so it applies to every caller.
- Pruning and undo never change the Focus Mode flag, even when a branch ends and another takes over. The "keep Focus Mode when switching branches" preference does not apply, because this is not a branch switch the user asked for (B4). `Workspace::set_active_keeping_presentation` exists for this.

### Undo puts the removed panels back (B5)

`Workspace::prune` returns a `PrunedSegment` holding:
- the removed panels, with their ids and full state;
- the removed panels' position among the destination's children;
- the branch record;
- the cursor the adopting branch had before the prune.

`Workspace::restore_pruned` puts all of that back, so `[`/`]` order is unchanged, and makes the pruned branch active again at its old panel.

Restoring is refused, and changes nothing, if the workspace has changed there since:
- the destination is gone;
- a panel for the same folder has been opened under it;
- the branch has grown again or been closed.

Ids are never reused, so restored ids cannot clash.

Undo deliberately does not restore a snapshot of the whole workspace. That would also reverse unrelated changes made after the prune.

`WorkspaceService` keeps the last 20 changes (`UNDO_LIMIT`) as `WorkspaceChange::Prune`. A change that can no longer be undone is dropped, so the next `u` reaches the one before it. The history is session-only and is not saved.

Undo leaves Focus Mode and the jump list alone. It undoes workspace changes only, never file operations. For that reason it is not bound to Ctrl+Z, which undoes file operations in GNOME Files.

### Selections (B6)

The selection in a removed panel leaves with the panel. The status bar says how many items were deselected, and undo restores them.

Running operations and the clipboard hold absolute paths, so they are unaffected.

### Keys (B6, B7, B8)

- **Bindings:** `prune-back` = `db`, `prune-back-segment` = `dB`, `undo-workspace` = `u`. All are configurable. `d` is an ordinary prefix in the existing keymap, like `g` and `z`.
- **Incomplete or invalid sequences:** `d<Esc>`, `dd`, `dj` and `d<Del>` are cancelled, and the second key is swallowed. So `d<Del>` can never reach the Trash shortcut.
- **Stale pending keys:** a pending `d` is dropped when a pointer action hits the workspace, when focus moves to another panel, or when the window loses focus. A later `b` cannot complete it in a different context.
- **Visual Mode:** `db` and `dB` give `Intent::NotInThisMode` and a hint (B6). `d` is still captured, so in Visual Mode, unlike Vim, it never deletes anything.
- **Command Mode:** keys keep going to the command line.
- **Override precedence (B8):** these three actions' defaults give way to a user binding that overlaps them. The default is dropped with a warning, instead of the user's binding being rejected. Conflicts with every other action are handled as before.

### Layers

- **Domain:** `workspace/prune.rs` holds `PruneMotion`, `resolve_prune` (pure), `prune`, `restore_pruned`, `PruneRefusal` and `RestoreRefusal`.
- **Application:** `WorkspaceService::prune_back` and `undo_workspace`, plus `workspace/undo.rs`. The only effects are `Unwatch` for removed panels and `Persist`. The service has no filesystem port.
- **Interaction:** the three actions and the Visual Mode rule.
- **Presentation:** it calls the service and phrases the outcome in the status bar.
- **Infrastructure:** unchanged. The workspace file format stays at version 2.

## Consequences

- `db` and `dB` match spec §4/§5 only where the tree has a fork at the right place. For example, `dB` lands on Documents when Documents also leads to another branch. On a path without forks, `db` removes one panel and `dB` goes back to the root.
- Spec §11 asks for pruning to work in Focus Mode. Decision B9 turns it off there instead. STATUS.md records this.
- Undo applies only to prunes. `close_branch` could add a `WorkspaceChange` later.
- **Tests:**
  - Domain: the spec scenarios, every refusal, the protection of shared ancestors and other branches, Focus Mode, and undo round trips and conflicts. The prune-and-undo operations are added to a randomised invariant run.
  - Application: the effects, Focus Mode refusals, the undo history, and that running operations and files are untouched.
  - Interaction: sequences, modes and override precedence.
  - The workspace UI test: prune and undo on a real session, with a fingerprint check that no file changed.
