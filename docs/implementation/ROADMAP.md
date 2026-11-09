# Roadmap

## Original specification (0.1.0)

All six phases of [the original specification](../specifications/0-INITIAL-PRODUCT-SPEC.md) §17 are complete. See [definition-of-done.md](../definition-of-done.md).

## Spatial file manager

The phases follow the approved plan, which refines [the spatial specification](../specifications/1-SPATIAL-FILE-MANAGER.md) §17. Product decisions D1–D19 are in [DECISIONS.md](../architecture/DECISIONS.md), and the architecture is in [ADR 0004](../decisions/0004-spatial-workspace.md).

| Phase | Scope | Status |
|---|---|---|
| 0. Groundwork | Spec and instruction files, decision log, view types moved to the domain (D19), search no longer captures typing (D1) | ✅ Done |
| 1. Workspace and panels | Domain panel tree with invariants; `WorkspaceService` and effects; per-panel listings, watches and LRU; workspace strip; `h`/`j`/`k`/`l`, `gg`, `G`, `Enter`, `Backspace` | ✅ Done |
| 2. Branching | First-class branches, shared ancestors, switch and close, branch bar, fork menus, jump list (D2), sidebar and address semantics (D3), unavailable panels (D14) | ✅ Done |
| 3. Focus Mode | Presentation flag; navigation and branch switching while focused; retention preference; toolbar toggle; indicator; neighbour hints | ✅ Done |
| 4. Interaction modes | Normal, Visual and Command; configurable keymap with conflict detection; command line with history and completion; mode bar; help | ✅ Done |
| 5. Cross-panel operations | Workspace-wide selection; explicit destinations (paste, `:copy`/`:move`, picker, drop); confirmation across panels (D7); clipboard staging (D9) | ✅ Done |
| 6. Persistence and refinement | Versioned workspace store; restore on launch; UI end-to-end test of the definition of done | ✅ Done; refinements open (see below) |
| 7. Branches side by side | Grid layout (depth across, branches down), shared ancestors spanning rows, connectors, compact cards for other branches, overflow stubs, short-window fallback ([ADR 0005](../decisions/0005-branch-layout.md), decisions L1–L4) | ✅ Done |
| 8. Command-line completion | Folder paths for `:copy`, `:move` and `:open`, branch targets, smartcase, Tab and Shift+Tab cycling (decisions P1–P4) | ✅ Done |
| 9. Performance run | Instrumentation, benchmarks at 50/200/1,000 branches, end-to-end UI run, saving off the UI thread ([review](../review/performance.md), decisions R1–R4) | ✅ Done; two follow-ups open |

### Open refinements

See [STATUS.md](STATUS.md) for detail.

- Performance follow-ups: listing cache size for many branches (D13), incremental model updates for very large folders.
- Manual checks:
  - drag and drop between panels
  - touchpad horizontal scrolling
  - GNOME and KDE sessions
