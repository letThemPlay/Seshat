# Feature Specification: Fuzzy Find

## 1. Overview

Introduce an interactive, fzf-inspired fuzzy search system that allows users to quickly locate files and directories without manually navigating through the filesystem.

The feature should provide fast, incremental, keyboard-first searching with ranked results, query matching, live filtering and direct navigation to results.

Fuzzy Find must integrate with the existing spatial workspace and branching navigation model. It must not replace or disrupt the current navigation workflow.

The implementation should follow the existing Clean Architecture, Rust conventions and architectural decisions documented in the project's foundational specification.

## 2. Goals

- Find files and directories quickly using partial or incomplete names.
- Support fuzzy matching rather than requiring exact substrings.
- Rank results by relevance.
- Search incrementally as the user types.
- Navigate results entirely using the keyboard.
- Support mouse interaction.
- Search within the current directory or a wider scope.
- Open results without losing existing workspace state.
- Integrate with existing file-operation workflows.
- Remain responsive when searching large directories.
- Provide predictable behaviour across multiple workspace branches.

## 3. Non-goals

The initial implementation does not need to provide:

- Full-text content indexing.
- Semantic or AI-powered search.
- A background filesystem indexer.
- A replacement for the operating system's file search.
- Shell command execution.
- Arbitrary command execution through the search interface.
- Automatic modification of files.

These can be considered separately in future specifications.

## 4. User interaction

### 4.1 Opening Fuzzy Find

Provide a configurable keyboard shortcut to open Fuzzy Find.

Suggested default:
- `Ctrl+P`

The shortcut should be configurable and must not conflict with existing application shortcuts.

When opened:
- Display a search interface over the current workspace.
- Focus the search input immediately.
- Preserve the current active panel.
- Preserve the current branch and workspace state.
- Do not create or activate a new panel.
- Begin searching according to the currently selected search scope.

Fuzzy Find should be accessible from all interaction modes where appropriate, including Normal Mode and Focus Mode.

If Visual Selection is active, opening Fuzzy Find must not silently discard the selection. The implementation must define and test whether the selection is suspended and restored or whether the shortcut is unavailable during that mode.

### 4.2 Search interface

The interface should contain:

1. Search input.
2. Search scope indicator or selector.
3. Ranked result list.
4. Optional result count.
5. Loading or search-in-progress indication.
6. Keyboard shortcut hints where useful.

The search interface should be visually consistent with the application's theme.

It should be compact and avoid unnecessarily obscuring the workspace.

### 4.3 Incremental search

Search results should update as the user types.

Requirements:
- No explicit Enter key should be needed to begin searching.
- Query changes should trigger a new search.
- Previous results should be replaced or updated.
- The currently highlighted result should remain stable where possible.
- Empty queries should display a sensible default state rather than unexpectedly listing the entire filesystem.
- Search should support Unicode filenames.
- Matching should be case-insensitive by default.
- Matching should support partial names and non-contiguous character sequences.

Example:

Query: `dcm`

Potential matches:
- `Documents`
- `Development/Code/Modules`
- `Downloads/Camera`

The ranking system should favour results that best match the query.

## 5. Matching and ranking

Use an established Rust fuzzy-matching library where practical instead of implementing an entirely new matching algorithm.

Evaluate `nucleo`, which is used by modern fuzzy-finding tools, alongside suitable alternatives.

The matcher should support:

- Character subsequence matching.
- Consecutive character bonuses.
- Filename-prefix bonuses.
- Word-boundary bonuses.
- Case-sensitive match bonuses where applicable.
- Path matching.
- Match-position scoring.
- Deterministic tie-breaking.

### 5.1 Ranking

Results should be ranked primarily by fuzzy-match quality.

Additional ranking considerations may include:
- Filename match quality.
- Directory-name match quality.
- Exact filename matches.
- Prefix matches.
- Match location within the path.
- Stable tie-breaking.

Do not let arbitrary filesystem metadata dominate match relevance.

The ranking behaviour should be documented and covered by tests.

### 5.2 Match highlighting

Matched characters should be highlighted in result names.

Where useful, matching portions of the parent path may also be highlighted.

Highlighting must:
- Work with Unicode filenames.
- Preserve correct character boundaries.
- Remain readable across supported themes.
- Not alter the underlying filename representation.

## 6. Search scopes

Implement search scopes as an extensible concept.

Initial scopes:

### 6.1 Current Directory

Search the contents of the active panel's directory.

- Include files and directories.
- Search recursively.
- Respect configured ignore rules.
- Avoid following symbolic links by default.
- Include hidden files only when configured or explicitly requested.

### 6.2 Current Branch

Search within the active branch's navigable directory context.

The scope should be clearly defined: it should cover the active branch's relevant directory tree, not accidentally include unrelated workspace branches.

### 6.3 Entire Workspace

Search across the roots represented by all open branches.

Requirements:
- Do not search the same underlying directory repeatedly merely because it appears as a shared ancestor.
- Preserve the originating branch and panel context for each result.
- Clearly identify which branch or path produced a result.
- Do not silently search unrelated filesystem locations.

### 6.4 Absolute Path

Allow users to search or navigate using an absolute path.

This may be implemented as a separate path-entry behaviour rather than normal fuzzy matching.

The application must validate the path and report unavailable or inaccessible locations gracefully.

## 7. Result presentation

Each result should display:

- Filename.
- Parent directory or relevant path context.
- File or directory type indicator.
- Optional icon or thumbnail when available.
- Match highlighting.

Results should be visually distinguishable between files and directories.

The interface should support a large result set without rendering every result simultaneously.

Use list virtualisation or an equivalent approach.

Results must have stable identities. Do not rely on display names alone because different directories can contain files with identical names.

## 8. Keyboard interaction

Suggested default behaviour:

| Key | Action |
|---|---|
| `Ctrl+P` | Open Fuzzy Find |
| `Esc` | Close Fuzzy Find |
| `Up` | Previous result |
| `Down` | Next result |
| `Enter` | Open selected result |
| `Tab` | Optional preview or result detail action |
| `Ctrl+Enter` | Optional open in new branch |
| `Ctrl+O` | Optional open containing directory |
| `Ctrl+Backspace` | Clear query or remove previous query segment |

These shortcuts are proposed defaults and must be checked against the existing keyboard model before implementation.

The search input must retain normal text-editing behaviour. Arrow keys should navigate results only when focus and interaction state make that behaviour unambiguous.

## 9. Opening results and workspace integration

This is a critical part of the feature.

### 9.1 Opening a directory

When the selected result is a directory:
- Navigate to that directory using the existing spatial navigation system.
- Create or activate the appropriate panel and branch according to existing workspace rules.
- Do not destroy unrelated branches.
- Preserve the user's previous workspace state.
- Close the search interface after successful navigation.

### 9.2 Opening a file

When the selected result is a file:
- Open it using the application's existing file-opening mechanism.
- Do not assume every file type can be opened internally.
- Use the existing desktop integration strategy where applicable.
- Preserve the workspace and active branch.

### 9.3 Open in a new branch

Where supported, allow users to open a directory result in a new independent branch.

The operation must:
- Preserve existing branches.
- Reuse shared ancestors logically.
- Avoid creating duplicate logical representations of shared panels.
- Assign stable identifiers to new workspace objects.

### 9.4 Reveal containing directory

Provide an action to reveal a result in its containing directory.

The application should:
- Activate or create the relevant branch.
- Navigate to the parent directory.
- Select the matching file or directory.
- Restore the panel's normal navigation state.

## 10. File operations from search results

Search results should eventually integrate with Visual Selection and the central operation manager.

Users should be able to select one or more results and perform supported operations such as:
- Copy.
- Move.
- Rename.
- Delete.
- Add to selection.

Where practical, allow selected search results to participate in existing cross-panel and cross-branch operations.

Requirements:
- Revalidate source paths before starting operations.
- Handle files that have disappeared or changed.
- Do not bypass the central operation manager.
- Report operation progress through the existing operation event system and D-Bus interface.
- Do not perform destructive operations simply because a search result was selected.

If multi-result operations materially increase complexity, deliver them in a later phase rather than weakening the initial search implementation.

## 11. Performance requirements

Fuzzy Find must feel immediate.

Requirements:
- Search incrementally.
- Avoid blocking the UI thread.
- Use background tasks for filesystem enumeration and matching where appropriate.
- Cancel or supersede obsolete searches.
- Avoid spawning unbounded concurrent searches.
- Debounce input only where it improves performance without making interaction feel sluggish.
- Stream or progressively publish results when beneficial.
- Limit rendered results using virtualisation.
- Cache reusable information only where cache invalidation can be handled safely.

The implementation must account for:
- Large directory trees.
- Slow disks.
- Network-mounted directories.
- Permission-denied directories.
- Filesystem changes during searches.
- Application shutdown during an active search.

Do not introduce a persistent index in the initial implementation unless profiling demonstrates that it is necessary.

### Suggested performance targets

These are initial engineering targets, not guarantees:

- Query updates should not block UI interaction.
- Small-directory results should appear within approximately 100 ms under normal local conditions.
- Search cancellation should be prompt.
- The interface should remain responsive while scanning large trees.
- Memory use should be bounded for very large result sets.

Measure performance using representative test datasets rather than relying on theoretical estimates.

## 12. Filesystem behaviour and safety

- Do not follow symbolic links by default.
- Avoid filesystem traversal loops.
- Respect permission boundaries.
- Handle broken symbolic links.
- Respect configured hidden-file behaviour.
- Respect configured ignore patterns.
- Handle files disappearing during enumeration.
- Handle inaccessible mount points.
- Do not assume paths are UTF-8.
- Preserve native filesystem path representations internally.
- Do not execute files as part of searching.
- Do not silently cross filesystem or mount boundaries unless the selected scope allows it.

Search must never modify filesystem contents.

## 13. Architecture

Follow Clean Architecture.

Suggested responsibilities:

### Domain

Define:
- SearchQuery.
- SearchScope.
- SearchResult.
- SearchMatch.
- SearchSession.
- SearchStatus.

The domain should not depend on the UI framework or a particular fuzzy-matching library.

### Application

Provide use cases such as:
- StartSearch.
- UpdateSearchQuery.
- CancelSearch.
- GetSearchResults.
- OpenSearchResult.
- RevealSearchResult.

The application layer coordinates search sessions, cancellation and result publication.

### Infrastructure

Provide:
- Filesystem traversal.
- Fuzzy matcher adapter.
- Ignore-rule handling.
- Filesystem change detection where needed.

The fuzzy-matching library must be isolated behind an adapter so it can be replaced without affecting domain or UI code.

### Presentation

Responsible for:
- Search input.
- Result rendering.
- Selection movement.
- Match highlighting.
- Scope selection.
- Keyboard and mouse interactions.

Do not place filesystem traversal, ranking algorithms or workspace mutations inside UI components.

### Workspace integration

Search must integrate through existing workspace use cases and services.

It must not maintain an independent competing model of:
- Branches.
- Panels.
- Panel history.
- File operations.

## 14. Search lifecycle and concurrency

Every search session should have a stable identity.

When a query changes:
- The previous search should be cancelled or superseded.
- Late results from an older query must not overwrite newer results.
- Search status should distinguish idle, searching, completed, cancelled and failed states.
- Errors should be reported without crashing the application.

The active search session must not outlive its owning context unnecessarily.

## 15. Configuration

Provide configurable settings for:

- Keyboard shortcut.
- Default search scope.
- Include hidden files.
- Ignore patterns.
- Follow symbolic links (default off).
- Maximum displayed results.
- Search case sensitivity.
- Result preview visibility.

Avoid exposing excessive configuration before the feature has established useful defaults.

## 16. Accessibility and theming

- Search interface must follow the active theme.
- Match highlighting must remain legible.
- Keyboard focus must be visible.
- Result selection must be visually clear.
- Do not communicate file types through colour alone.
- Support scalable text and normal application accessibility behaviour.

## 17. Testing

### Unit tests

Test:
- Fuzzy matching.
- Ranking.
- Match highlighting.
- Unicode matching.
- Case handling.
- Search scopes.
- Stable result identities.
- Cancellation.
- Result ordering.
- Ignore patterns.

### Integration tests

Test:
- Recursive traversal.
- Permission errors.
- Symlinks.
- Large directory trees.
- Network and unavailable mounts where practical.
- Filesystem changes during search.
- Branch-aware result handling.
- Search cancellation and superseded queries.

### UI tests

Test:
- Opening and closing Fuzzy Find.
- Incremental query updates.
- Keyboard result navigation.
- Mouse selection.
- Opening files.
- Opening directories.
- Revealing containing directories.
- Focus Mode integration.
- Branch switching.
- Preserving workspace state.

### Regression tests

Verify that:
- Existing spatial navigation continues to work.
- Existing panel history remains intact.
- Branches are not destroyed or duplicated.
- Focus Mode behaves as specified.
- Existing file operations still use the central operation manager.
- D-Bus operation progress remains accurate.

## 18. Implementation phases

### Phase 1: Core fuzzy matching
- Evaluate Rust matching libraries.
- Implement matcher adapter.
- Implement ranking and highlighting.
- Add unit tests.

### Phase 2: Interactive search
- Implement search session lifecycle.
- Implement current-directory search.
- Add incremental UI.
- Add cancellation and result streaming.
- Add keyboard navigation.

### Phase 3: Spatial workspace integration
- Add current-branch search.
- Add entire-workspace search.
- Preserve branch and panel context.
- Implement open and reveal actions.
- Test shared ancestors and branch switching.

### Phase 4: Refinement
- Add configuration.
- Add performance profiling.
- Add advanced result actions.
- Evaluate multi-result file operations.
- Improve previews and accessibility.

## 19. Acceptance criteria

The feature is complete when:

1. Users can open Fuzzy Find using a configurable shortcut.
2. Search begins incrementally as the user types.
3. Partial and non-contiguous filename matches work.
4. Results are ranked by relevance.
5. Matched characters are highlighted correctly.
6. Search runs without blocking UI interaction.
7. Users can navigate results using the keyboard and mouse.
8. Current-directory search works reliably.
9. Search cancellation prevents stale results from appearing.
10. Opening a result integrates with existing spatial navigation.
11. Existing branches and panel state remain intact.
12. Workspace search identifies the originating branch and path.
13. Search handles inaccessible and changing filesystem entries gracefully.
14. Existing architectural and platform requirements remain satisfied.
15. Automated tests cover matching, traversal, cancellation and workspace integration.

## 20. Instructions for implementation

Before writing code:

1. Read the root CLAUDE.md.
2. Read the foundational product specification.
3. Read the spatial workspace specification.
4. Inspect the current repository and existing spatial workflow PoC.
5. Identify existing search, navigation, workspace and filesystem abstractions.
6. Identify existing keyboard modes and shortcut conventions.
7. Evaluate `nucleo` and alternatives against project requirements.
8. Produce an implementation plan and identify architectural implications.

Do not rebuild existing workspace or navigation systems.

Do not introduce a second file-operation manager.

Do not silently change existing specifications or architectural decisions.

Do not begin implementation until the proposed plan has been reviewed and approved.

Implement in small phases, with tests and documentation updated alongside the code.

## 21. Definition of done

Fuzzy Find is a first-class feature of the spatial file manager, not a disconnected search dialog.

It must be fast, predictable, keyboard-first, filesystem-safe and fully integrated with the persistent branching workspace.
