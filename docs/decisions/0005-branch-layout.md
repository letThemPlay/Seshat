# ADR 0005: Branches side by side with connectors

- **Status:** Accepted
- **Date:** 2026-11-09
- **Spec references:** [spatial file manager](../specifications/1-SPATIAL-FILE-MANAGER.md) §3.3, §3.8, §4.4, §10.2, §10.5, §14; decisions L1–L4 in [DECISIONS.md](../architecture/DECISIONS.md)

## Context

[ADR 0004](0004-spatial-workspace.md) drew only the active branch, as a single strip of panels. Other branches could be reached through the branch bar and fork menus, but you couldn't see them. The spec asks for shared ancestors shown once, branch-specific panels grouped, parent–child relationships apparent through subtle connectors, and inactive branches visible or easily discoverable.

## Decision

The workspace is a grid.

- **Columns are tree depth.** A panel's column depends only on its depth, so switching branches never moves panels sideways (§4.4).
- **Rows are branches,** in the same depth-first order as `[` and `]`. In that order every panel's branches sit in adjacent rows, so a **shared ancestor is one cell spanning them** and is never drawn twice.
- **The active branch's row expands,** and other branches are compact lanes. The rows stay where they are while the active branch is in view (L3).

How it is built:

- **Layout model:** `presentation::workspace_layout` computes cells (column, row, span, full or card), connector edges and the shown window of rows from the `Workspace`. It is pure, with no GTK. Tests include a randomised check that every panel spans exactly the rows of the branches passing through it.
- **Full panels and cards:** panels on the active branch within three columns of the active panel are full `PanelView`s, as before. Everything else is a `BranchCard`, drawn from the panel's saved state (folder, cursor item, selection count, branch number). Showing a branch never reads its folders, so the listing cache limit (D13) still holds (L1).
- **Connectors:** a lane between columns draws elbow lines from a parent's header to each child's header. Inactive lines use the theme's `border` token and the active path the `accent` token, each through CSS, so themes style them like everything else.
- **Number of rows:** up to six branch rows are shown, fewer in short windows. When no lane fits, the layout is a single strip. Branches beyond the shown rows collapse into "+N branches" stubs that open the branch overview (L2, L4). The setting `[workspace] layout = "strip"` always uses the single strip.
- **Grid:** `gtk::Grid` provides the row spans. Widgets are pooled per panel and only re-attached when their cell changes. If the active panel is re-attached, it gets keyboard focus back.
- **Focus Mode** still shows only the active panel, hiding the rest without destroying it.

## Consequences

- Inactive branches are visible with their cursor and selection, without extra filesystem reads or memory.
- Vertical space is shared: the active branch keeps at least 300 px, and each lane takes 70 px.
- The domain, the application services, the operation manager and D-Bus are unchanged.
- The UI test checks the layout through a `workspace layout` debug line (rows, columns, hidden branches, cards, connectors), including the short-window fallback.
