# Feature Specification: Spatial Branching File Manager

## 1. Product vision

### 1.1 Overview

Transform the file manager into a spatial, keyboard-first filesystem workspace.

Instead of replacing the current directory whenever a user navigates, the application presents directories as persistent panels arranged in a horizontally scrolling workspace.

The interaction model combines:

- **Niri-inspired navigation:** spatial navigation through a horizontally scrolling workspace.
- **Vim-inspired interaction:** keyboard-first navigation, modes, commands and visual selection.
- **Persistent navigation:** directory panels retain their state when the user moves elsewhere.
- **Branching navigation:** multiple directory paths coexist independently.
- **Focus Mode:** expand the active panel to fill the workspace without losing access to neighbouring panels.
- **Cross-panel operations:** files can be selected and operated on across different panels and branches.

The central idea:

> The filesystem should feel like a persistent spatial workspace that users move through, rather than a collection of pages they repeatedly open and close.

### 1.2 Core principles

1. Navigation should be spatial and predictable.
2. Opening a directory must not unexpectedly destroy another navigation path.
3. Multiple branches must coexist independently.
4. Every panel must preserve its state.
5. Focus Mode must change presentation, not navigation behaviour.
6. Keyboard interaction is the primary interaction method.
7. Mouse interaction must remain fully supported.
8. File operations must work across panels and branches.
9. Workspace state must be independent of UI component lifetimes.
10. The application must remain responsive with large workspaces and directories.

---

## 2. Terminology

| Term | Definition |
|---|---|
| Workspace | The complete environment containing panels and branches |
| Panel | A persistent view of a single directory |
| Branch | A navigation path through the directory hierarchy |
| Active branch | The branch currently receiving navigation input |
| Active panel | The directory panel currently receiving file navigation input |
| Shared ancestor | A directory panel referenced by multiple branches |
| Selected file | The current navigation cursor |
| Visual selection | One or more files explicitly selected for an operation |
| Focus Mode | A presentation mode in which the active panel fills the workspace |
| Normal Workspace Mode | The standard multi-panel workspace presentation |

---

## 3. Workspace and branching model

### 3.1 Core behaviour

The workspace consists of persistent directory panels connected through a branching navigation structure.

Opening a directory creates or activates a child panel.

If a user opens a different directory from an existing panel, the application creates another child branch. It must not silently replace the existing child.

All existing branches remain available until explicitly closed by the user.

### 3.2 Example: separate directory branches

Starting at Home:

1. Open Documents.
2. Open Projects inside Documents.
3. Return to Home.
4. Open Downloads.

The workspace now contains:

```text
                         WORKSPACE
                             |
                            Home
                             |
                +------------+------------+
                |                         |
             Documents                 Downloads
                |
             Projects
```

Documents and Downloads are separate branches.

Projects remains attached to Documents.

Downloads must not become a child of Projects.

### 3.3 Shared ancestors

Branches originating from the same directory should share that ancestor logically.

For example:

- Home is a shared ancestor.
- Documents belongs to one branch.
- Projects belongs to the Documents branch.
- Downloads belongs to another branch.

Shared ancestors must not be duplicated unnecessarily in the underlying workspace model.

The UI may render shared ancestors once, while visually communicating their relationship to multiple branches.

### 3.4 Branch creation

When opening a directory:

1. Resolve the directory's identity.
2. Check whether it is already represented as a child in the current navigation context.
3. If it exists, activate the existing panel and associated branch.
4. Otherwise, create a new child panel.
5. Attach the panel to the current panel in the navigation tree.
6. Activate the resulting branch.
7. Preserve all existing branches.

Do not create unnecessary duplicate panels for the same directory under the same parent context.

### 3.5 Branch persistence

Branches remain open until explicitly closed.

Switching branches must:
- Restore the branch's active panel.
- Restore the active panel's selected file.
- Restore scroll positions.
- Restore sorting and view settings.
- Restore search filters.
- Restore expanded metadata.
- Preserve valid visual selections.
- Preserve navigation history.

Switching branches must not reconstruct or reset their panels.

### 3.6 Branch switching

Provide:
- Keyboard branch switching.
- Mouse branch switching.
- A visible active-branch indicator.
- A branch overview or selector.
- Clear parent-child relationships.

Suggested default bindings:

| Key | Action |
|---|---|
| `[` | Previous branch |
| `]` | Next branch |

These bindings must be configurable.

### 3.7 Closing branches

Users must be able to explicitly close a branch.

Closing a branch:
- Removes branch-specific panels when they are no longer referenced.
- Preserves shared ancestors.
- Preserves panels referenced by other branches.
- Selects a sensible remaining branch.
- Never deletes files from the filesystem.

An operation already in progress must not be invalidated simply because its originating branch is closed.

### 3.8 Branch visualisation

The workspace should communicate the branching structure clearly.

Recommended design:
- Shared ancestors appear in the common navigation area.
- Branch-specific panels are visually grouped.
- Subtle connectors communicate parent-child relationships.
- Branches have identifiable labels or paths.
- The active branch has a stronger visual highlight.
- Inactive branches remain visible or easily discoverable.

The underlying branching model must remain independent of the rendering strategy.

---

## 4. Spatial navigation

### 4.1 Horizontal navigation

Horizontal movement navigates between panels within the active branch.

Suggested defaults:

| Key | Action |
|---|---|
| `h` | Previous panel |
| `l` | Next existing panel |

Requirements:
- Moving left activates the previous panel in the active branch.
- Moving right activates the next existing panel.
- Moving towards a shared ancestor activates the existing ancestor panel.
- Opening a directory is a separate action from moving right.
- Horizontal movement must not unexpectedly create branches.

### 4.2 Vertical navigation

Vertical movement navigates the contents of the active panel.

Suggested bindings:

| Key | Action |
|---|---|
| `j` | Move down |
| `k` | Move up |
| `gg` | First item |
| `G` | Last item |
| `Ctrl+d` | Scroll down |
| `Ctrl+u` | Scroll up |

Requirements:
- Selected item remains visible.
- Selection is visually distinct.
- Large directories remain responsive.
- Scrolling and selection behave predictably.
- Beginning and end-of-list behaviour is configurable.

### 4.3 Opening and returning

Suggested defaults:

| Key | Action |
|---|---|
| `Enter` | Open selected directory or activate selected file |
| `Backspace` | Activate parent panel |

Returning to a parent activates the existing parent panel.

It must not recreate the panel, reset its state or destroy its child branches.

### 4.4 Workspace scrolling

The workspace scrolls horizontally to keep the active panel visible.

Requirements:
- Smooth transitions.
- Predictable panel positioning.
- No unnecessary jumps when switching branches.
- Support large workspaces.
- Avoid rendering every panel at full cost.
- Keep the active panel visually prominent.
- Support touchpad and mouse horizontal scrolling.

---

## 5. Panel state and history

### 5.1 Persistent panel state

Every panel maintains its own state independently.

| Property | Description |
|---|---|
| Selected file | Current navigation cursor |
| Vertical scroll position | Current file-list position |
| Sort order | Sort field and direction |
| View mode | List, grid or other presentation |
| Search filter | Active query and filter settings |
| Expanded metadata | Expanded details and sections |
| Visual selection | Selected files and range anchor |
| Hidden files | Visibility preference |
| Column configuration | Column widths and visibility |
| Preview state | Preview configuration, if supported |

### 5.2 State restoration

When activating a panel, restore its previous state.

If the selected file no longer exists:
- Select the nearest valid item where possible.
- Otherwise clear the selection.
- Never leave the panel in an invalid state.

If directory contents change externally:
- Refresh the listing.
- Preserve selection where possible.
- Preserve sorting and filtering.
- Update scroll position sensibly.
- Remove stale selections where their files no longer exist.

### 5.3 Panel lifecycle

Panel state must not depend on UI widget lifetime.

A panel can be:
- Active and rendered.
- Inactive and rendered.
- Inactive and temporarily not rendered.
- Restored later without losing state.

UI virtualization must not destroy logical panel history.

---

## 6. Focus Mode

### 6.1 Purpose

Focus Mode allows the user to expand the active directory panel to fill the available workspace.

It provides a distraction-free view of the current directory while retaining the spatial navigation model.

**Focus Mode is a presentation mode, not a separate navigation mode.**

The underlying workspace, branches, panels and relationships remain unchanged.

### 6.2 Core behaviour

When Focus Mode is activated:

- The active panel expands to fill the available workspace area.
- Neighbouring panels are hidden from the main workspace view.
- Hidden panels remain logically available.
- Existing branches remain intact.
- The active panel retains its selected file and scroll position.
- All panel history is preserved.
- The user can still navigate to neighbouring panels.
- The user can still switch branches.

Focus Mode must never destroy, recreate or duplicate panels.

### 6.3 Navigation while focused

Horizontal navigation remains fully functional.

For example:

1. The user is focused on Documents.
2. Pressing `l` activates Projects.
3. Projects expands to fill the workspace.
4. Pressing `h` returns to Documents.
5. Documents expands to fill the workspace again.

This must work without exiting Focus Mode.

Similarly, branch switching remains available:

1. The user is focused on Projects.
2. They switch to the Downloads branch.
3. Downloads becomes the active panel.
4. Downloads occupies the full workspace.

The user remains in Focus Mode throughout.

### 6.4 Entering and exiting Focus Mode

Provide a configurable keyboard shortcut to toggle Focus Mode.

Suggested default:

- `zz` in Normal Mode toggles Focus Mode.

The shortcut must be configurable and must not conflict with other application actions.

When exiting Focus Mode:
- Restore the multi-panel workspace presentation.
- Restore the active panel's original position in the workspace.
- Preserve the active branch.
- Preserve all panel state.
- Do not reload directories unnecessarily.

### 6.5 Visual behaviour

In Focus Mode:
- The active panel occupies the available workspace width.
- Neighbouring panels are hidden from the main content area.
- The active directory path remains visible.
- Branch identity remains accessible.
- A subtle Focus Mode indicator is displayed.
- The user can still discover neighbouring panels through navigation hints or a branch/panel overview.

Avoid adding a separate navigation interface that makes Focus Mode feel like a different application.

### 6.6 Mouse interaction

Focus Mode must support mouse interaction.

Users should be able to:
- Navigate files using the mouse.
- Open directories.
- Access branch switching.
- Exit Focus Mode using a visible control or menu action.
- Navigate to neighbouring panels through available navigation controls.

### 6.7 Interaction modes

Focus Mode must work alongside:
- Normal Mode.
- Command Mode.
- Visual Selection Mode.

Focus Mode must not override or reset the current interaction mode.

### 6.8 Focus Mode persistence

Define a workspace-level preference controlling whether Focus Mode is retained when switching branches.

The default should be to retain Focus Mode across branch switches.

Focus Mode must not be treated as panel-specific state unless explicitly introduced as a future preference.

### 6.9 Acceptance criteria

1. Activating Focus Mode expands the active panel.
2. Neighbouring panels are hidden but remain logically available.
3. Horizontal navigation works while focused.
4. Branch switching works while focused.
5. The active panel changes when navigating.
6. Exiting Focus Mode restores the multi-panel layout.
7. All panel and branch state remains intact.
8. No unnecessary directory reloads occur.
9. Keyboard and mouse navigation both work.
10. Focus Mode works in all interaction modes.

---

## 7. Interaction modes

### 7.1 Normal Mode

Normal Mode is the default interaction mode.

It supports:
- File navigation.
- Panel navigation.
- Branch switching.
- Opening directories.
- Returning to parents.
- Entering Visual Selection Mode.
- Entering Command Mode.
- Toggling Focus Mode.
- Invoking common actions.

Suggested default bindings:

| Key | Action |
|---|---|
| `h` | Previous panel |
| `l` | Next panel |
| `j` | Move down |
| `k` | Move up |
| `Enter` | Open selected directory or file |
| `Backspace` | Parent panel |
| `[` | Previous branch |
| `]` | Next branch |
| `v` | Visual Selection Mode |
| `:` | Command Mode |
| `zz` | Toggle Focus Mode |
| `Esc` | Cancel transient interaction |

Bindings must be configurable.

### 7.2 Command Mode

Command Mode provides a command-line interface within the application.

Requirements:
- Visible command prompt.
- Command execution.
- Command history.
- Command completion where practical.
- Clear validation errors.
- Cancellation using `Esc`.
- Consistent return to Normal Mode.

Suggested commands:

| Command | Purpose |
|---|---|
| `:copy` | Copy selected files |
| `:move` | Move selected files |
| `:delete` | Delete selected files |
| `:rename` | Rename an item |
| `:mkdir` | Create a directory |
| `:search` | Search |
| `:sort` | Change sorting |
| `:view` | Change view mode |
| `:hidden` | Toggle hidden files |
| `:branch` | Show or switch branches |
| `:branch close` | Close active branch |
| `:focus` | Toggle Focus Mode |
| `:help` | Show commands |

All commands must use the same application services as keyboard shortcuts and mouse actions.

### 7.3 Visual Selection Mode

Visual Selection Mode provides Vim-inspired multi-file selection.

Requirements:
- Explicit entry and exit.
- Clear visual indication.
- Selection anchor.
- Moving the cursor extends or changes the selection range.
- Support individual and range selection.
- Distinguish the active item from selected items.
- Support files and directories.
- Allow selections to be used in cross-panel operations.

Suggested bindings:

| Key | Action |
|---|---|
| `v` | Toggle selection of current item |
| `V` | Enter range selection |
| `j` / `k` | Extend selection |
| `Esc` | Exit Visual Selection Mode |

Selection semantics must be documented and tested.

### 7.4 Mode transitions

Requirements:
- Display the current mode prominently.
- `Esc` has consistent behaviour.
- Modal input must not accidentally trigger Normal Mode navigation.
- Mouse interaction remains functional.
- Dialogs and command inputs receive appropriate keyboard focus.
- Focus Mode must not interfere with interaction mode transitions.

---

## 8. File operations across panels and branches

### 8.1 Purpose

Users must be able to operate on files from different panels and branches in a single workflow.

This is a core differentiator of the application.

### 8.2 Supported scenarios

Examples:
- Copy files from Documents into Downloads.
- Move files from Projects into another branch.
- Select files from multiple directories and copy them into one destination.
- Compare source and destination directories.
- Copy a directory between branches.
- Perform operations without changing the active branch.

### 8.3 Selection model

Visual selections must be independent of the active panel.

An operation can contain files from:
- One panel.
- Multiple panels in one branch.
- Multiple branches.
- Shared ancestor panels and branch-specific panels.

Every selected item must retain its original source path.

Do not resolve source paths using the active panel at execution time.

### 8.4 Destination selection

Destination selection must be explicit.

Supported methods:
- Select a destination panel and invoke paste.
- Use a command targeting a specific panel or branch.
- Drag selected files into another panel.
- Use a destination picker.

The UI must make clear:
- Which items are sources.
- Which location is the destination.
- Which operation will happen.
- Whether the operation is copy or move.

### 8.5 Operation lifecycle

Integrate with the existing central file operation manager.

The operation manager remains the single source of truth.

Each operation tracks:
- Unique operation ID.
- Operation type.
- Source paths.
- Destination path.
- Item count.
- Current item where applicable.
- Progress.
- Lifecycle status.
- Errors.
- Conflicts.
- Cancellation status.

Changing branches, entering Focus Mode or closing a panel must not change an operation that has already started.

### 8.6 Operation feedback

Display:
- Active operations.
- Progress.
- Completed operations.
- Failed operations.
- Conflicts.
- Cancellation controls.

Continue using the existing D-Bus progress interface so external applications can observe operations.

The workspace UI and external clients must consume the same operation state.

### 8.7 Safety and conflicts

- Never silently overwrite files.
- Present clear conflict-resolution options.
- Confirm destructive actions where appropriate.
- Handle missing sources and unavailable destinations.
- Support cancellation where possible.
- Preserve source selections and workspace state after errors.
- Ensure operations cannot accidentally target the wrong branch.

---

## 9. Mouse interaction

The application must remain fully usable with a mouse.

Requirements:
- Click a panel to activate it.
- Click a branch to activate it.
- Click a file to select it.
- Double-click or activate a directory to open it.
- Drag and drop between panels.
- Support multi-selection.
- Support horizontal scrolling.
- Show hover, active, selected and focused states.
- Provide accessible controls for common actions.
- Provide a visible Focus Mode toggle.

Mouse interactions must update the same workspace state and invoke the same application services as keyboard interactions.

---

## 10. Visual design and layout

### 10.1 Overall layout

The application should feel like a spatial workspace rather than a conventional file manager with a directory tree and one file list.

Core elements:
- Location/navigation sidebar.
- Horizontally scrolling panel workspace.
- Branch indicators.
- Active panel highlight.
- Mode/status bar.
- Operation status display.
- Focus Mode indicator.

### 10.2 Normal Workspace Mode

The normal presentation displays multiple directory panels.

Requirements:
- Shared ancestors are identifiable.
- Branch-specific panels are visually grouped.
- Parent-child relationships are apparent.
- Active panel is clearly distinguished.
- Inactive panels remain discoverable.
- Panel boundaries are subtle.

### 10.3 Focus Mode

The active panel occupies the available workspace.

Requirements:
- Hide neighbouring panels from the main view.
- Keep branch and navigation controls accessible.
- Display the active directory path.
- Preserve the current selection and scroll position.
- Allow immediate navigation to neighbouring panels.
- Provide a clear indication that Focus Mode is active.

### 10.4 Panel states

Keep these states visually distinct:
- Active panel.
- Inactive panel.
- Active branch.
- Inactive branch.
- Selected file.
- Visual selection range.
- Hovered item.
- Focused command input.

Continue using the existing theme token architecture.

Preserve the established styling requirement:
- Selected navigation background uses the same background token as the main content area.
- Selected navigation item has 12px rounded corners.

### 10.5 Responsive behaviour

- Active panel remains usable at different window sizes.
- Inactive panels may be partially visible.
- Active panel receives sufficient space.
- Horizontal navigation remains predictable.
- Branch relationships remain understandable at narrow window sizes.
- Provide a branch overview when simultaneous visibility becomes impractical.

---

## 11. Search and filtering

Search filters belong to individual panels by default.

Requirements:
- Each panel remembers its own search query.
- Clearing a filter restores the normal directory listing.
- Selection remains stable where possible.
- Sorting and filtering work together predictably.
- Searching one panel must not unexpectedly change another panel.
- Branch switching restores the relevant search state.
- Focus Mode does not change search semantics.

Future consideration: workspace-wide search across all open branches.

Workspace-wide search should be a separate feature rather than silently changing panel-level search behaviour.

---

## 12. Workspace data model and architecture

The workspace must be represented as application state, not inferred from the rendered UI.

### 12.1 Suggested conceptual model

**Workspace**
- Workspace ID.
- Collection of panel nodes.
- Collection of branch definitions or navigation paths.
- Active branch ID.
- Active panel ID.
- Workspace scroll state.
- Focus Mode state.
- Workspace preferences.

**Panel**
- Stable panel ID.
- Directory identity and path.
- Parent panel ID.
- Child panel IDs.
- Panel state.
- Directory listing reference.
- Reference count or equivalent ownership information.

**Branch**
- Stable branch ID.
- Branch name or display label.
- Ordered navigation path.
- Current active panel ID.
- Creation time.
- Last active time.

**Panel State**
- Selected item.
- Scroll position.
- Sort configuration.
- View mode.
- Search filter.
- Expanded metadata.
- Visual selection.
- Column configuration.
- Preview configuration.

**Interaction State**
- Current mode.
- Current keyboard focus.
- Command input and history.
- Visual selection anchor.
- Active branch.

**Operation**
- Operation ID.
- Operation type.
- Source paths.
- Destination path.
- Progress and lifecycle state.

### 12.2 Shared ancestor representation

Use stable panel IDs and explicit relationships.

Where multiple branches share an ancestor, they should reference the same logical panel.

Do not duplicate shared ancestors because they appear in multiple branch paths.

When a branch closes, shared ancestors must remain while referenced by another branch or the workspace root.

### 12.3 Navigation service

Implement workspace navigation as an application service.

It should support:
- Activate panel.
- Activate branch.
- Move to parent.
- Move to child.
- Open directory.
- Create branch.
- Switch branch.
- Close branch.
- Resolve an existing child panel.
- Retrieve a branch path.
- Restore panel state.
- Toggle Focus Mode.

### 12.4 Presentation service

Focus Mode and Normal Workspace Mode must be presentation concerns.

The presentation layer should:
- Read the active workspace presentation state.
- Render the active panel appropriately.
- Preserve the underlying panel and branch model.
- Animate presentation transitions without changing navigation relationships.
- Restore the normal layout when Focus Mode is disabled.

Do not implement Focus Mode by creating a second copy of the active panel.

### 12.5 Clean Architecture

Maintain the existing Clean Architecture boundaries.

**Domain**
- Workspace concepts.
- Panel and branch relationships.
- Navigation rules and invariants.
- Workspace presentation state where appropriate.

**Application**
- Navigation use cases.
- Branch lifecycle.
- Panel restoration.
- Selection management.
- Operation orchestration.
- Focus Mode state changes.

**Infrastructure**
- Filesystem access.
- Directory monitoring.
- Workspace persistence.
- Platform integrations.

**Presentation**
- Workspace layout.
- Panels.
- Keyboard handling.
- Mouse interaction.
- Branch visualisation.
- Focus Mode rendering.
- Mode and operation feedback.

Do not put branch management or filesystem logic inside UI widgets.

---

## 13. Workspace persistence

The application should preserve workspace state across restarts.

Requirements:
- Persist open branches.
- Persist panel relationships.
- Persist active branch and panel.
- Persist selected items where valid.
- Persist scroll positions.
- Persist sorting and view modes.
- Persist filters and expanded metadata.
- Persist workspace presentation preference, including Focus Mode if configured.
- Restore the workspace on startup.
- Handle directories that no longer exist.
- Handle inaccessible paths gracefully.

Workspace persistence must be versioned so the data model can evolve safely.

Consider a user preference to restore the previous workspace on launch or start with a clean workspace.

---

## 14. Performance

The workspace may contain many panels, branches and large directory listings.

Requirements:
- Virtualize large file lists.
- Avoid rendering every panel at full cost.
- Preserve logical state when a panel is not rendered.
- Avoid unnecessary filesystem reloads.
- Keep horizontal navigation responsive.
- Keep branch switching responsive.
- Make Focus Mode transitions efficient.
- Update only affected panels when filesystem changes occur.
- Avoid blocking the UI during filesystem operations.
- Handle external filesystem changes.
- Avoid excessive memory usage from inactive branches.

Focus Mode should be an efficient presentation change, not a reason to reload the active directory.

---

## 15. Error handling and edge cases

Handle these cases explicitly:

- A directory is deleted while its panel is open.
- A directory is renamed externally.
- A directory becomes inaccessible.
- A selected file disappears.
- A source file changes during an operation.
- A destination branch is closed while an operation is running.
- A shared ancestor is involved in multiple branches.
- Two branches reference the same directory.
- A branch is closed while it contains visual selections.
- A workspace is restored with missing directories.
- A directory is opened from a panel with multiple existing child branches.
- Focus Mode is activated while the active panel is being refreshed.
- The active panel is closed while Focus Mode is enabled.
- A branch is switched while Focus Mode is enabled.
- The window is resized while Focus Mode is enabled.

The application must not silently destroy unrelated branches or lose operation context.

---

## 16. Testing and acceptance criteria

### 16.1 Branching

- Opening Documents from Home creates a child panel.
- Opening Projects from Documents creates a child panel.
- Returning to Home and opening Downloads creates a separate branch.
- Projects remains available.
- Documents remains available.
- Downloads does not become a child of Projects.
- Both branches can be switched between.
- Both branches retain their panel histories.
- Shared ancestors are represented correctly.
- Closing one branch does not destroy another branch's shared ancestors.

### 16.2 Navigation

- Horizontal movement changes panels within the active branch.
- Vertical movement changes the selected file.
- Opening a directory creates or activates the appropriate child.
- Returning to a parent activates the existing panel.
- Moving between branches restores the previous location.
- Navigation never silently replaces an existing branch.
- Key bindings are configurable.

### 16.3 Panel history

- Selected file is restored.
- Scroll position is restored.
- Sort order is restored.
- View mode is restored.
- Search filter is restored.
- Expanded metadata is restored.
- Visual selection is restored where valid.
- External filesystem changes are handled gracefully.

### 16.4 Focus Mode

- Activating Focus Mode expands the active panel to fill the workspace.
- Neighbouring panels are hidden but remain logically available.
- Horizontal navigation works while focused.
- Branch switching works while focused.
- The active panel changes when navigating.
- Exiting Focus Mode restores the multi-panel layout.
- All panel and branch state remains intact.
- No unnecessary directory reloads occur.
- Keyboard and mouse navigation both work.
- Focus Mode works in all interaction modes.
- Window resizing does not break the focused layout.

### 16.5 Interaction modes

- Normal Mode is the default.
- Command Mode works consistently.
- Visual Selection Mode supports range selection.
- Mode changes are visually obvious.
- Escape behaviour is consistent.
- Keyboard input is not accidentally routed to the wrong mode.
- Mouse interaction remains functional.

### 16.6 Cross-branch operations

- Files can be selected from multiple branches.
- Files can be copied between branches.
- Files can be moved between branches.
- Multiple sources can share a destination.
- Destination selection is explicit.
- Changing the active branch does not alter an operation.
- Changing Focus Mode does not alter an operation.
- Closing a branch does not invalidate an already-running operation.
- Progress is consistent with the central operation manager.
- Conflicts and failures do not lose selection context.

### 16.7 Persistence

- Open branches survive application restart.
- Panel state is restored.
- Focus Mode preference is restored when enabled.
- Invalid paths are handled gracefully.
- Workspace schema changes are versioned and migrated safely.

### 16.8 Performance

- Large directories remain responsive.
- Many panels do not cause excessive rendering overhead.
- Inactive panels can be virtualized without losing state.
- Branch switching does not cause unnecessary filesystem reloads.
- Focus Mode transitions do not reload directory contents.

---

## 17. Implementation phases

### Phase 1: Workspace and panel foundation

- Review the existing proof of concept.
- Implement workspace state.
- Implement panel state.
- Implement parent-child panel relationships.
- Implement horizontal and vertical navigation.
- Implement persistent panel history.
- Add automated navigation tests.

### Phase 2: Branching model

- Implement first-class branch representation.
- Implement shared ancestor relationships.
- Implement branch creation.
- Implement branch switching.
- Implement branch closing.
- Implement branch-aware visual layout.
- Test the Home → Documents → Projects and Home → Downloads scenario.

### Phase 3: Focus Mode

- Implement workspace presentation state.
- Implement active-panel expansion.
- Hide neighbouring panels without destroying them.
- Preserve navigation while focused.
- Support branch switching while focused.
- Implement restoration of the multi-panel layout.
- Add keyboard and mouse controls.
- Test all interaction modes.

### Phase 4: Interaction modes

- Implement Normal Mode.
- Implement Command Mode.
- Implement Visual Selection Mode.
- Implement configurable key bindings.
- Add mode indicators.
- Ensure keyboard and mouse use the same application services.

### Phase 5: Cross-panel operations

- Implement selection across panels and branches.
- Implement explicit destination selection.
- Implement copy and move.
- Integrate with the existing operation manager.
- Display progress and errors.
- Test branch switching during operations.

### Phase 6: Persistence and refinement

- Persist workspace and branch state.
- Restore workspace on startup.
- Add drag-and-drop.
- Improve branch visualisation.
- Improve search and filtering.
- Optimize large workspaces.
- Add comprehensive integration tests.

---

## 18. Instructions to Claude Code

Before implementation:

1. Review the existing proof of concept and architecture.
2. Identify existing components and services that can be reused.
3. Propose a workspace, panel and branch data model.
4. Explain how shared ancestors will be represented.
5. Document branch creation, switching and closing behaviour.
6. Document Focus Mode and how it interacts with the workspace model.
7. Document keyboard interaction and mode transitions.
8. Identify architectural changes required.
9. Present the proposed UI layout and interaction flow.
10. Implement incrementally with tests for each phase.

Do not replace working infrastructure without a clear technical reason.

Do not implement branches as visual-only groupings. They must be first-class application state.

Do not make the active branch the owner of all workspace state. Multiple branches must coexist independently.

Do not tie panel state to UI widget lifetimes.

Do not resolve file operation sources or destinations using whichever panel happens to be active when an operation executes.

Do not implement Focus Mode by duplicating panels or changing branch relationships.

Continue using the existing central file operation manager and D-Bus progress reporting.

Preserve the application's Linux-first and native Wayland requirements.

---

## 19. Definition of done

The feature is complete when a user can:

1. Open Home.
2. Open Documents.
3. Open Projects inside Documents.
4. Return to Home.
5. Open Downloads.
6. See both navigation branches preserved.
7. Switch between Projects and Downloads without losing panel state.
8. Continue navigating either branch independently.
9. Activate Focus Mode to expand the active panel.
10. Navigate to neighbouring panels without leaving Focus Mode.
11. Switch branches while remaining in Focus Mode.
12. Return to the multi-panel workspace without losing state.
13. Select files across different branches.
14. Copy or move those files into an explicitly chosen destination panel.
15. Observe operation progress without losing workspace context.

**The defining product requirement: multiple filesystem navigation paths must coexist as persistent, independently navigable branches within one spatial workspace, with the ability to focus on any panel without sacrificing spatial navigation.**
