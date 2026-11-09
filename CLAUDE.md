# Project Instructions for Claude Code

## 1. Project priorities

This is a Linux-first, native Wayland file manager built using Rust.

The existing proof of concept is working. Extend the existing application rather than unnecessarily rebuilding it.

The project has two primary product specifications:

- `docs/specifications/0-INITIAL-PRODUCT-SPEC.md`
- `docs/specifications/1-SPATIAL-FILE-MANAGER.md`

Both specifications must be respected.

## 2. Specification hierarchy

The original product specification establishes the foundational architecture, platform requirements and existing product behaviour.

The spatial file manager specification introduces additional functionality.

The new feature specification supplements the original specification. It does not replace, weaken or override existing requirements.

If requirements appear to conflict:

1. Identify the conflicting requirements.
2. Explain the technical or product implications.
3. Propose a resolution.
4. Ask for approval before changing an established requirement.

Never silently discard an existing requirement.

## 3. Non-negotiable architectural rules

- Maintain Clean Architecture boundaries.
- Keep domain and application logic independent of UI widgets.
- Keep filesystem operations outside the presentation layer.
- Preserve the existing working architecture unless a change is justified.
- Prefer extending existing services over duplicating functionality.
- Keep application state independent of UI component lifetimes.
- Use stable identifiers for workspace panels and branches.
- Keep navigation state separate from presentation state.
- Keep file-operation state separate from workspace navigation state.

## 4. Linux and Wayland requirements

- Linux is the primary target.
- Native Wayland operation is mandatory.
- XWayland fallback does not satisfy the native Wayland requirement.
- Preserve the existing platform integration decisions.
- Respect Wayland security and portal requirements.
- Maintain compatibility with the targeted GNOME and KDE environments.

Refer to `docs/architecture/LINUX-WAYLAND.md` for detailed requirements.

## 5. File operation requirements

- The central file operation manager remains the single source of truth.
- UI and external applications must observe the same operation lifecycle.
- Preserve the existing D-Bus progress reporting interface.
- Do not introduce a second independent operation lifecycle.
- File operations must not depend on whichever panel happens to be active.
- Source and destination paths must be captured explicitly.
- Handle errors, conflicts and cancellation safely.

## 6. Spatial workspace requirements

When implementing the spatial file manager:

- Branches must be first-class application state.
- Shared ancestors must be represented correctly.
- Multiple navigation branches must coexist.
- Switching branches must preserve panel history.
- Panel state must survive UI component disposal and recreation.
- Focus Mode changes presentation only.
- Focus Mode must never destroy or duplicate panels.
- Navigation must remain available while Focus Mode is active.
- Cross-branch file operations must work independently of the active branch.

Refer to `docs/specifications/1-SPATIAL-FILE-MANAGER.md` for complete requirements.

## 7. Development process

Before implementing a feature:

1. Read the relevant product specifications.
2. Inspect the existing implementation.
3. Identify existing services and components that can be reused.
4. Identify architectural implications.
5. Propose an implementation plan.
6. Identify risks and unresolved design decisions.
7. Implement incrementally.
8. Add or update automated tests.
9. Run the relevant tests and build checks.
10. Update documentation and implementation status.

For substantial features, use Plan Mode first and wait for approval before making implementation changes.

Do not begin a large implementation based on assumptions.

## 8. Code quality

- Follow existing Rust conventions and project patterns.
- Prefer small, testable application services.
- Avoid unnecessary dependencies.
- Avoid duplicated business logic.
- Handle errors explicitly.
- Do not introduce silent fallback behaviour that violates product requirements.
- Keep performance and responsiveness in mind.
- Document important architectural decisions.

## 9. Documentation maintenance

Keep these documents up to date:

- `docs/architecture/ARCHITECTURE.md`
- `docs/architecture/DECISIONS.md`
- `docs/architecture/LINUX-WAYLAND.md`
- `docs/implementation/ROADMAP.md`
- `docs/implementation/STATUS.md`

Update documentation when implementation changes an established decision or introduces a new architectural pattern.

Never silently change the product specifications to match an implementation shortcut.

## 10. Completion requirements

Before declaring a feature complete:

- Verify all relevant acceptance criteria.
- Run relevant automated tests.
- Run build and lint checks.
- Confirm existing functionality has not regressed.
- Confirm original architectural and platform requirements remain satisfied.
- Update implementation status.
- Report incomplete requirements honestly.

When uncertain, ask rather than inventing a product decision.
