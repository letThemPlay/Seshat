# Seshat File Explorer
## Technical Product Specification & Implementation Blueprint

**Project:** Seshat File Explorer  
**Primary language:** Rust  
**Target platform:** Linux  
**Display protocol:** Native Wayland  
**Architecture:** Clean Architecture  
**IPC:** D-Bus  
**Development approach:** Incremental, test-driven implementation

---

# 1. Project Overview

Build a modern, polished Linux desktop file explorer using Rust.

The application must follow Clean Architecture principles, SOLID principles, idiomatic Rust practices and established software engineering best practices.

The application should provide a fast, intuitive and visually polished file management experience while maintaining a modular, testable and extensible internal architecture.

A fundamental requirement is that file operation management is independent of the user interface.

All file operations must be tracked by a centralised operation management system. This system must expose operation state and progress through IPC, allowing other applications, such as desktop bars and status panels, to independently monitor ongoing file operations.

The application must operate natively under Wayland without requiring XWayland.

The UI must be fully themeable, supporting built-in themes and custom user-defined themes.

## 1.1 Core objectives

- Native Linux desktop application.
- Native Wayland operation.
- Rust-first implementation.
- Clean Architecture.
- Modular and extensible design.
- Asynchronous filesystem operations.
- Reliable file operation management.
- D-Bus IPC integration.
- Externally consumable progress reporting.
- Fully themeable UI.
- Modern, minimal interface.
- Robust error handling.
- Comprehensive automated testing.
- Long-term maintainability.

## 1.2 Target desktop environments

Primary targets:

- GNOME on Wayland.
- KDE Plasma on Wayland.

Other compatible Wayland compositors should be supported where practical.

X11 may be considered as a secondary compatibility target, but native Wayland must remain the primary implementation.

---

# 2. Technology Selection and Architecture Decision

Claude Code must evaluate the available Rust UI frameworks before selecting one.

Do not automatically select a framework based on popularity or familiarity.

The selected framework must satisfy the application's Linux, Wayland, customisation, accessibility and maintainability requirements.

## 2.1 Frameworks to evaluate

Evaluate:

1. GTK4 with gtk-rs / Relm4.
2. Iced.
3. Tauri.
4. Slint.

## 2.2 Evaluation criteria

| Criterion | Priority |
|---|---|
| Native Wayland support | Critical |
| Linux desktop integration | Critical |
| Rust integration | Critical |
| Custom UI styling | Critical |
| Filesystem integration | Very high |
| Asynchronous operations | Very high |
| Accessibility | High |
| Performance | High |
| Memory consumption | High |
| Packaging | High |
| Long-term maintenance | High |
| Development complexity | Medium |

## 2.3 Native Wayland evaluation

For every framework, establish:

- Whether it creates native Wayland surfaces.
- Whether it requires XWayland.
- Whether it can operate without an X11 server.
- Which Wayland protocols it supports.
- Whether clipboard functionality works natively.
- Whether drag and drop works natively.
- Whether file dialogs work correctly.
- Whether fractional scaling is supported.
- Whether multi-monitor configurations work.
- Whether keyboard shortcuts work correctly.
- Whether window decorations and resizing behave correctly.
- Whether GNOME and KDE Plasma behave consistently.

Do not select a framework that requires XWayland for normal application operation.

## 2.4 Required architecture decision record

Create an ADR documenting:

- Framework comparison.
- Technical advantages and disadvantages.
- Native Wayland implementation details.
- Rendering architecture.
- Linux dependencies.
- Accessibility considerations.
- Custom styling capabilities.
- Runtime performance.
- Packaging considerations.
- Licensing considerations.
- Final framework recommendation.

The recommendation must be supported by actual documentation and a small proof of concept.

## 2.5 Proof of concept

Before committing to the framework, build a minimal working prototype containing:

- Main application window.
- Left navigation pane.
- Main content pane.
- Theme switching.
- Selected navigation item styling.
- Native Wayland rendering.
- A representative asynchronous UI update.
- Basic file selection or directory enumeration.

Verify that the prototype actually runs as a Wayland client.

Do not proceed to the full implementation until the framework choice has been documented.

---

# 3. Architecture

Use Clean Architecture with clear separation between domain, application, infrastructure and presentation.

## 3.1 Suggested project structure

```text
file-explorer/
│
├── crates/
│   ├── domain/
│   │   ├── entities/
│   │   ├── value_objects/
│   │   ├── repositories/
│   │   └── errors/
│   │
│   ├── application/
│   │   ├── use_cases/
│   │   ├── services/
│   │   ├── events/
│   │   ├── dto/
│   │   └── ports/
│   │
│   ├── infrastructure/
│   │   ├── filesystem/
│   │   ├── ipc/
│   │   ├── persistence/
│   │   ├── linux/
│   │   └── logging/
│   │
│   └── presentation/
│       ├── components/
│       ├── views/
│       ├── state/
│       ├── theme/
│       └── navigation/
│
├── tests/
│   ├── integration/
│   └── fixtures/
│
├── docs/
│   ├── architecture/
│   ├── ipc/
│   ├── decisions/
│   └── development/
│
├── Cargo.toml
└── README.md
```

This structure is a starting point.

Claude Code should determine which components warrant separate crates and which should remain modules within a crate.

Avoid creating unnecessary crate boundaries.

## 3.2 Domain layer

Owns:

- File and directory entities.
- File metadata models.
- Operation entities.
- Operation identifiers.
- Operation lifecycle.
- Progress value objects.
- Domain errors.
- Repository abstractions.
- Domain invariants.

The domain must not depend on:

- UI frameworks.
- D-Bus.
- Tokio.
- Concrete filesystem implementations.
- Platform-specific presentation APIs.

## 3.3 Application layer

Owns:

- Application use cases.
- File operation orchestration.
- Operation management.
- Progress aggregation.
- Cancellation coordination.
- Application event definitions.
- Repository ports.
- Application-level validation.

All file operations must be initiated through application use cases.

## 3.4 Infrastructure layer

Owns concrete implementations of:

- Filesystem repositories.
- Linux filesystem integration.
- D-Bus service.
- Persistence.
- Configuration loading.
- Logging.
- Platform-specific integrations.

Infrastructure must implement application-defined interfaces rather than being directly coupled to the presentation layer.

## 3.5 Presentation layer

Owns:

- Main window.
- Navigation.
- File listings.
- Toolbar.
- Context menus.
- UI state.
- Theme rendering.
- User interaction.
- Progress presentation.

Presentation must never perform filesystem operations directly.

It should invoke application use cases and observe application state.

## 3.6 Dependency rules

- Domain depends on nothing outside itself.
- Application depends on domain.
- Infrastructure implements application and domain abstractions.
- Presentation consumes application functionality.
- Dependencies must point inwards.
- Use explicit dependency composition.
- Prefer strongly typed models.
- Use traits at meaningful architectural boundaries.
- Avoid unnecessary abstraction.

---

# 4. Linux Filesystem Integration

Use established Linux and freedesktop.org conventions.

Do not reinvent functionality that is already provided by established desktop infrastructure.

## 4.1 Standard directories

Resolve standard user directories using XDG user directory conventions.

Support:

- Home.
- Desktop.
- Documents.
- Downloads.
- Pictures.
- Music.
- Videos.
- Trash.

Do not hardcode directory paths.

Do not assume standard directories use English names or that all directories exist.

## 4.2 Storage devices

Support:

- Local filesystems.
- Mounted volumes.
- Removable storage.
- External drives.
- Network-mounted filesystems where supported.

Investigate GIO and its volume monitoring capabilities.

Handle:

- Mount events.
- Unmount events.
- Device removal.
- Unavailable devices.
- Filesystem permission changes.

## 4.3 Trash

Implement compatibility with the freedesktop.org Trash specification.

Support:

- Moving files to Trash.
- Browsing Trash.
- Restoring files.
- Permanently deleting files.
- Handling trash on different filesystems.
- Handling unavailable trash locations.

Never silently delete a file if moving it to Trash fails.

## 4.4 Filesystem correctness

- Support symbolic links.
- Handle inaccessible directories.
- Handle non-UTF-8 filenames.
- Handle large directories.
- Avoid unintended symbolic link traversal.
- Preserve metadata where appropriate.
- Handle filesystem permission errors.
- Handle removable storage disconnections.
- Handle differing filesystem capabilities.
- Avoid loading file contents unnecessarily.

Use `PathBuf` and `OsStr` internally.

Never assume filesystem paths are valid UTF-8.

---

# 5. File Explorer Functionality

## 5.1 File browsing

Support:

- Directory browsing.
- Directory navigation.
- Navigation history.
- Back and forward.
- Parent directory navigation.
- Refresh.
- Address/path navigation.
- Breadcrumb navigation.
- File metadata.
- Hidden file visibility.
- Sorting.
- Multi-selection.
- Search.

## 5.2 File operations

Implement:

- Copy.
- Move.
- Rename.
- Delete.
- Create directory.
- Create file.
- Restore from Trash.
- Permanently delete.
- Open files using the operating system's default application.
- Open directories.
- Copy paths to clipboard.

## 5.3 Conflict resolution

When copying or moving files, support:

- Overwrite.
- Skip.
- Rename destination.
- Cancel operation.

Provide appropriate confirmation for destructive operations.

Avoid silent data loss.

## 5.4 File presentation

Support:

- List view.
- Grid view.
- File and directory icons.
- File sizes.
- Modification dates.
- File types.
- Multi-selection.
- Context menus.
- Drag and drop.
- Empty states.
- Loading states.
- Error states.

---

# 6. User Interface

The UI should be modern, clean and minimal.

Prioritise clarity, usability and consistency.

It should feel like a native Linux desktop application.

## 6.1 Main layout

The primary interface consists of:

1. Top toolbar.
2. Navigation controls.
3. Search.
4. Left navigation pane.
5. Main content pane.
6. Optional bottom status area.

Conceptual layout:

```text
+------------------------------------------------------+
| Back | Forward | Path / Breadcrumbs       | Search  |
+-------------------+----------------------------------+
|                   |                                  |
| Navigation        |                                  |
|                   |                                  |
| Home              |                                  |
| Desktop           |                                  |
| Documents         |       Main Content               |
| Downloads         |                                  |
| Pictures          |       Files and folders          |
| Music             |                                  |
| Videos            |                                  |
|                   |                                  |
| Devices           |                                  |
|                   |                                  |
+-------------------+----------------------------------+
| Status / Selection Information                      |
+------------------------------------------------------+
```

The navigation pane must be resizable.

## 6.2 Left navigation pane

Display standard locations:

- Home.
- Desktop.
- Documents.
- Downloads.
- Pictures.
- Music.
- Videos.
- Trash.

Display mounted storage devices below standard locations.

Support custom bookmarks.

Use appropriate icons.

## 6.3 Selected navigation item – mandatory design requirement

This is a strict visual requirement.

When a navigation location is selected:

- Its background colour must be exactly the same as the main content pane background.
- Its border radius must be exactly 12px.
- All four corners must use this radius.
- The selected state must remain visually distinguishable.
- Use appropriate text, icon, subtle outline or other non-contrasting treatment.
- Do not introduce a contrasting filled background.

The selected item's background and main content background must reference the same theme token.

Example:

```text
theme:
  colors:
    surface: "#1E1E1E"

main_content:
  background: surface

navigation:
  selected:
    background: surface
    border_radius: 12px
```

The values are illustrative.

The 12px radius is mandatory.

The styling must work consistently in light and dark themes.

## 6.4 Main content pane

Requirements:

- Clean file and folder presentation.
- Consistent spacing.
- Appropriate icons.
- File selection.
- Multi-selection.
- Sorting.
- Context menus.
- Keyboard navigation.
- Drag and drop.
- Responsive resizing.

The main content background must use the shared surface theme token.

---

# 7. Theme Architecture

The application must be fully themeable.

All visual styling must use a centralised theme system.

Do not scatter hardcoded colours, spacing or typography throughout components.

## 7.1 Built-in themes

- Light.
- Dark.
- Follow system preference.

## 7.2 Custom themes

Support user-defined themes.

Prefer TOML or JSON for external theme configuration.

Themes must be:

- Validated.
- Loaded independently of UI components.
- Persisted between application sessions.
- Switchable at runtime.
- Extensible without changing component implementations.

Provide sensible defaults for missing or invalid values.

## 7.3 Theme tokens

### Colours

- Application background.
- Main content surface.
- Navigation surface.
- Primary text.
- Secondary text.
- Muted text.
- Accent.
- Border.
- Hover.
- Selected.
- Focus.
- Error.
- Warning.
- Success.

### Geometry

- Border radius.
- Padding.
- Spacing.
- Component heights.
- Icon dimensions.

### Typography

- Font family.
- Font size.
- Font weight.
- Line height.

## 7.4 Theme consistency

The selected navigation item's background and main content background must always reference the same token.

Do not duplicate colour definitions.

The 12px selected navigation radius must be enforced centrally.

---

# 8. File Operation Management

This is a fundamental architectural component.

Implement a centralised File Operation Manager.

Every file operation must pass through this manager.

It must be independent of both the presentation layer and IPC transport.

## 8.1 Supported operations

- Copy.
- Move.
- Rename.
- Delete.
- Create directory.
- Create file.
- Restore from Trash.
- Permanently delete.

## 8.2 Operation lifecycle

```text
Queued
  |
Started
  |
InProgress
  |
  +----> Completed
  |
  +----> Failed
  |
  +----> Cancelling
             |
             +----> Cancelled
             |
             +----> Completed
             |
             +----> Failed
```

Use strongly typed Rust enums.

Define valid transitions explicitly.

Prevent invalid lifecycle transitions.

## 8.3 Operation manager responsibilities

- Generate unique operation IDs.
- Track active operations.
- Track operation status.
- Track progress.
- Track errors.
- Coordinate cancellation.
- Publish lifecycle events.
- Maintain operation snapshots.
- Support multiple concurrent operations.
- Record final operation results.

## 8.4 Progress reporting

Where applicable, expose:

- Operation ID.
- Operation type.
- Current status.
- Source path.
- Destination path.
- Total files.
- Completed files.
- Total bytes.
- Completed bytes.
- Percentage.
- Current file.
- Start time.
- Elapsed time.
- Error details.

Support indeterminate progress when total work cannot be calculated.

Do not report misleading percentages.

Throttle frequent progress updates to avoid unnecessary IPC traffic while ensuring important lifecycle events are never lost.

## 8.5 Concurrency

- Use bounded concurrency.
- Avoid unbounded task creation.
- Ensure operations do not block the UI.
- Handle cancellation safely.
- Handle partial failures.
- Handle unavailable destinations.
- Handle insufficient storage.
- Handle disappearing source files.

## 8.6 Background operation lifecycle

Closing or hiding the explorer window must not unexpectedly terminate ongoing operations.

Initially, the operation manager may live within the main application process.

However, design its interfaces so it can eventually be moved into a dedicated background process without changing the domain or application layers.

Document the limitations of the initial process lifecycle.

---

# 9. IPC – D-Bus Integration

D-Bus is the primary IPC mechanism.

Investigate and use the Rust `zbus` library unless a compelling technical reason justifies another implementation.

IPC is a first-class application capability.

It must not be an afterthought or embedded in UI components.

## 9.1 Service identity

Proposed identity:

```text
Bus name:
com.example.FileExplorer

Object path:
/com/example/FileExplorer

Interface:
com.example.FileExplorer.Operations
```

Treat these as configurable project constants.

## 9.2 Required methods

Expose methods equivalent to:

```text
GetActiveOperations()
GetOperation(operation_id)
CancelOperation(operation_id)
GetProtocolVersion()
```

All requests must be validated.

Return structured errors.

## 9.3 Required signals

Expose:

```text
OperationQueued
OperationStarted
OperationProgress
OperationCancelling
OperationCompleted
OperationFailed
OperationCancelled
```

Signals must contain sufficient information to identify the operation and understand its current state.

## 9.4 Event format

Define strongly typed Rust structures and D-Bus serialisation contracts.

Example conceptual payload:

```json
{
  "protocol_version": 1,
  "sequence": 42,
  "operation": {
    "id": "uuid",
    "type": "copy",
    "status": "in_progress",
    "source": "/home/user/Documents",
    "destination": "/mnt/storage/Documents",
    "progress": {
      "files_total": 120,
      "files_completed": 45,
      "bytes_total": 104857600,
      "bytes_completed": 39321600,
      "percentage": 37.5
    }
  }
}
```

The actual D-Bus interface must use appropriate typed arguments and structures rather than assuming JSON is the transport format.

## 9.5 Consumer functionality

External applications must be able to:

1. Connect to the service.
2. Subscribe to operation signals.
3. Retrieve active operations.
4. Retrieve individual operation details.
5. Receive progress updates.
6. Detect completion, failure and cancellation.
7. Reconnect after disconnection.
8. Rebuild their UI state using an operation snapshot.

## 9.6 Snapshot consistency

Avoid race conditions between obtaining an operation snapshot and subscribing to events.

Use monotonically increasing sequence numbers.

Document the subscription and snapshot procedure.

Consumers must be able to identify missing events and resynchronise.

## 9.7 Security

- Use the session D-Bus.
- Restrict access to the user's desktop session.
- Validate incoming requests.
- Apply appropriate authorization.
- Enforce sensible message limits.
- Avoid exposing unnecessary filesystem information.
- Do not allow IPC requests to bypass application validation.

## 9.8 External example client

Provide a small Rust example demonstrating how another application can:

- Connect to the D-Bus service.
- Retrieve active operations.
- Subscribe to progress events.
- Display progress.
- Handle completion.
- Reconnect after service interruption.

The protocol must be language-independent so applications written in other languages can consume it.

---

# 10. Native Wayland Support – Mandatory

Native Wayland support is a release requirement.

The application must not rely on XWayland for normal operation.

An application running through XWayland does not satisfy this requirement.

## 10.1 Native operation

- Create native Wayland surfaces.
- Use supported Wayland rendering backends.
- Support Wayland window management.
- Support resizing and maximising.
- Support multiple monitors.
- Support HiDPI displays.
- Support fractional scaling where available.
- Handle display configuration changes.
- Support native keyboard interaction.
- Respect Wayland security restrictions.

Do not require an X11 server.

## 10.2 Wayland protocols

Investigate appropriate support for:

- `xdg-shell`.
- `xdg-decoration`.
- Wayland data-device protocols.
- Fractional scaling protocols.
- Output and display configuration.
- Appropriate desktop portals.

Use framework-provided integration wherever possible.

Do not bypass Wayland security restrictions or rely on unsupported compositor-specific behaviour.

## 10.3 Clipboard and drag and drop

Support:

- Copying file paths.
- Copying files between applications.
- Dragging files into the explorer.
- Dragging files out of the explorer.
- Moving files between directories.
- Keyboard clipboard shortcuts.

Use native Wayland integration and supported data-transfer protocols.

## 10.4 Desktop integration

Verify operation under:

- GNOME Wayland.
- KDE Plasma Wayland.
- Other compatible Wayland compositors where practical.

Account for differences between compositors and optional protocol support.

Provide sensible fallbacks when a capability is unavailable.

## 10.5 Actual Wayland runtime verification

This requirement is particularly important.

Claude Code must verify that the application is actually running as a native Wayland client.

Do not consider an application Wayland-compatible simply because it launches inside a Wayland desktop session.

The implementation must:

1. Identify the active display backend.
2. Confirm native Wayland connection.
3. Detect whether XWayland is being used.
4. Report the detected backend in diagnostic logs.
5. Include runtime verification in the testing process.

Where appropriate, inspect:

- `WAYLAND_DISPLAY`.
- `XDG_SESSION_TYPE`.
- The actual UI toolkit display backend.
- The actual compositor connection.

Environment variables alone are not sufficient proof.

Use framework or runtime diagnostics to verify the actual backend.

The application must fail the Wayland compliance test if it silently falls back to XWayland.

## 10.6 Wayland testing

Test:

- Native Wayland launch.
- GNOME Wayland.
- KDE Plasma Wayland.
- Multiple monitors.
- Different display scaling configurations.
- Clipboard operations.
- Drag and drop.
- Window resizing.
- Focus changes.
- File dialogs.
- Keyboard shortcuts.
- Desktop theme integration.

Record the detected backend in test output.

## 10.7 Definition of Done

The application must:

- Launch as a native Wayland client.
- Operate without requiring XWayland.
- Support standard Wayland window management.
- Support clipboard integration.
- Support drag and drop.
- Support GNOME and KDE Plasma Wayland.
- Handle HiDPI displays.
- Respect Wayland security boundaries.
- Pass automated and manual Wayland verification.

X11 support may be added later as a secondary compatibility target.

---

# 11. Application State

Separate application state from UI state.

The application layer owns authoritative operation state.

The presentation layer observes and renders it.

Maintain state for:

- Current directory.
- Navigation history.
- Selected files.
- Sorting.
- View mode.
- Bookmarks.
- Mounted devices.
- Theme.
- Preferences.
- Active file operations.

Avoid unnecessary duplication of filesystem state.

Persist appropriate user preferences.

---

# 12. Error Handling and Logging

Implement structured, typed error handling.

Requirements:

- Separate domain errors from infrastructure errors.
- Provide useful UI error messages.
- Log unexpected failures.
- Include relevant contextual information.
- Avoid logging sensitive file contents.
- Handle partial operation failures.
- Ensure individual failures do not crash the application.
- Include operation identifiers in relevant logs.

Use appropriate logging levels.

Provide enough diagnostic information to investigate filesystem, IPC and Wayland issues.

---

# 13. Testing Strategy

Testing must be integrated throughout development.

## 13.1 Unit tests

Test:

- Domain entities.
- Value objects.
- Operation lifecycle.
- State transitions.
- Progress calculations.
- Error handling.
- Theme validation.
- Use cases.

## 13.2 Integration tests

Test:

- Filesystem repository.
- Copy operations.
- Move operations.
- Rename operations.
- Delete operations.
- Recursive operations.
- Conflict resolution.
- Trash integration.
- IPC serialisation.
- IPC connection.
- IPC reconnection.
- Active operation snapshots.
- Event sequencing.

## 13.3 UI tests

Test:

- Navigation.
- Selection.
- Theme switching.
- Selected location styling.
- File listing.
- Multi-selection.
- Context menus.
- Operation progress presentation.

## 13.4 Linux environment testing

Test under:

- GNOME Wayland.
- KDE Plasma Wayland.
- X11 where practical for secondary compatibility.
- Different scaling configurations.
- Multiple monitors.

Verify the actual display backend.

Do not treat an XWayland session as a successful Wayland test.

## 13.5 IPC consumer testing

Create a test consumer that independently connects to the application and verifies:

- Operation discovery.
- Event subscriptions.
- Progress updates.
- Multiple simultaneous operations.
- Reconnection.
- Snapshot recovery.
- Terminal operation states.

---

# 14. Performance and Reliability

The application must:

- Remain responsive during large file operations.
- Handle large directories efficiently.
- Avoid unnecessary filesystem rescans.
- Avoid excessive UI updates.
- Use bounded concurrency.
- Avoid unbounded task creation.
- Avoid unnecessary memory allocations.
- Handle removable devices disappearing.
- Handle unavailable destinations.
- Shut down cleanly.
- Avoid resource leaks.

Do not sacrifice filesystem correctness for performance.

---

# 15. Documentation

Provide:

1. Project README.
2. Architecture documentation.
3. Architecture Decision Records.
4. Framework selection rationale.
5. Filesystem implementation documentation.
6. IPC protocol specification.
7. D-Bus interface documentation.
8. External IPC client example.
9. Theme development documentation.
10. Linux and Wayland setup instructions.
11. Build and packaging instructions.
12. Testing instructions.
13. Instructions for adding new file operations.

---

# 16. Packaging and Distribution

Design the application for Linux distribution.

Investigate:

- Flatpak.
- AppImage.
- DEB.
- RPM.

Prioritise one working packaging format initially.

Document:

- Build dependencies.
- Runtime dependencies.
- D-Bus service configuration.
- Desktop entry.
- Application icon.
- Wayland requirements.
- Required permissions.

Ensure packaging does not accidentally force the application to run through XWayland.

For Flatpak, explicitly investigate the required Wayland socket and filesystem permissions.

---

# 17. Development Phases

Implement incrementally.

### Phase 1 – Research and foundation

- Inspect repository.
- Evaluate UI frameworks.
- Create architecture decision record.
- Establish Rust workspace.
- Define domain and application boundaries.
- Build UI proof of concept.
- Verify native Wayland operation.

Deliverable: documented architecture and working application skeleton.

### Phase 2 – Filesystem integration

- Resolve XDG directories.
- Implement directory enumeration.
- Implement navigation.
- Implement mounted device discovery.
- Implement Trash integration.

Deliverable: working file browsing.

### Phase 3 – UI and theming

- Implement main window.
- Implement navigation pane.
- Implement main content pane.
- Implement list and grid views.
- Implement theme tokens.
- Implement light and dark themes.
- Implement custom themes.
- Enforce selected item styling.

Deliverable: polished file explorer interface.

### Phase 4 – File operations

- Implement operation manager.
- Implement copy.
- Implement move.
- Implement rename.
- Implement delete.
- Implement conflict resolution.
- Implement cancellation.
- Implement progress tracking.

Deliverable: reliable asynchronous file operations.

### Phase 5 – IPC

- Implement D-Bus service.
- Implement versioned protocol.
- Implement lifecycle signals.
- Implement progress signals.
- Implement operation snapshots.
- Implement reconnection.
- Build example external client.

Deliverable: independently consumable operation progress.

### Phase 6 – Quality and distribution

- Complete automated testing.
- Complete Wayland verification.
- Review security.
- Review performance.
- Review architecture.
- Implement packaging.
- Complete documentation.

Deliverable: release-ready Linux application.

---

# 18. Claude Code Implementation Instructions

Claude Code must work as an engineering agent, not simply a code generator.

## Before implementation

1. Inspect the existing repository.
2. Identify existing code and conventions.
3. Research current framework documentation.
4. Evaluate the candidate UI frameworks.
5. Verify Wayland support from actual framework documentation.
6. Evaluate D-Bus integration.
7. Present architecture and dependency recommendations.
8. Present the proposed project structure.
9. Document architectural risks.
10. Build the framework proof of concept.

Do not start by generating the entire application.

## During implementation

For each phase:

1. Explain the implementation plan.
2. Implement the functionality.
3. Write tests.
4. Run formatting checks.
5. Run Clippy.
6. Run automated tests.
7. Resolve compilation warnings and errors.
8. Update documentation.
9. Verify previous functionality remains operational.

Use:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```

Adapt commands where workspace features or platform dependencies require it.

## Engineering standards

- Follow idiomatic Rust.
- Prefer established dependencies.
- Avoid unnecessary dependencies.
- Avoid giant files.
- Keep components focused.
- Use descriptive names.
- Avoid unnecessary abstractions.
- Document architectural decisions.
- Do not leave core functionality as placeholders.
- Do not silently omit requirements.
- Do not weaken security or filesystem correctness.
- Do not compromise native Wayland requirements.

## Definition of Done

The application is complete only when:

- It builds and runs on Linux.
- It runs as a native Wayland client without requiring XWayland.
- Its actual display backend has been verified.
- Standard locations resolve correctly.
- File browsing works.
- Core file operations work.
- The UI remains responsive.
- Multiple operations can execute simultaneously.
- Operation state is centrally managed.
- Progress is exposed through D-Bus.
- An independent application can consume progress.
- IPC consumers can reconnect and recover state.
- Light and dark themes work.
- Custom themes work.
- Selected navigation background matches the main content background.
- Selected navigation border radius is exactly 12px.
- Filesystem errors are handled safely.
- Automated tests pass.
- Documentation is complete.

**Final architectural principle:**

The File Operation Manager is the single source of truth for all file operation state.

The UI and external IPC consumers must independently observe the same operation lifecycle and progress.

Native Wayland operation is mandatory and must be verified at runtime. An application running through XWayland does not meet this requirement.

These requirements must not be weakened to simplify implementation.
