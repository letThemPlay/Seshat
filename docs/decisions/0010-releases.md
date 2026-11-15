# ADR 0010: Releases through a release pull request

- **Status:** Accepted
- **Date:** 2026-11-15
- **References:** decisions R1–R7 in [DECISIONS.md](../architecture/DECISIONS.md#releases-plan-approved-2026-11-15), [releasing.md](../development/releasing.md); builds on [ADR 0003](0003-packaging.md)

## Context

Seshat had never been released: no tags, no GitHub Releases, and the version recorded differently in `Cargo.toml`, the changelog and the AppStream metadata. The maintainer asked for automated releases that update the changelog and version, with alpha, beta and rc pre-releases, and chose to keep the hand-written changelog and to release through a pull request.

Users install Seshat from the Nix flake, pinned by git ref, so a release is a tag and its notes; nothing is published to crates.io.

## Decision

1. **One version, Semantic Versioning, alpha/beta/rc pre-releases (R1).** Every crate inherits `[workspace.package] version`.
2. **The changelog stays hand-written (R2).** Pull requests add to `## Unreleased`; a release renames that section.
3. **Our own scripts, cargo-release for the bump only (R3, R4).** `scripts/release/prepare.sh` bumps the version with `cargo release version`, stamps the changelog and adds the AppStream release; `to-publish.sh` and `notes.sh` decide and describe a publication. All are tested in CI on a throwaway copy, and an untested cargo-release series stops them.
4. **A release pull request, opened by a GitHub App (R5, R6).** The "Prepare release" workflow, started by hand with a level, runs `prepare.sh` and opens `chore(release): VERSION` as the release App, whose PRs run CI like any other. The maintainer reviews and merges it.
5. **Publishing on merge (R7).** "Publish release" runs on every push to `main` but only acts when the push changed the version and that version has no tag: it then tags the merged commit `vVERSION` and publishes the GitHub Release with the changelog section, as a pre-release for alpha, beta and rc.

## Alternatives considered

- **release-please, release-plz, semantic-release, cocogitto, knope:** all generate the changelog from commits, which the maintainer did not want; release-please also can't rewrite an inherited `version.workspace`, release-plz has no pre-release levels and is built around crates.io, and semantic-release commits to `main` without review.
- **A release button that commits and tags directly:** quicker, but the release commit would skip review and CI and need a branch-protection bypass.
- **`GITHUB_TOKEN` for the release PR:** PRs it opens trigger no workflows, so the required checks would never run. A personal access token would act as the maintainer, who then couldn't approve the PR.

## Consequences

- A release is: run "Prepare release", review and merge the PR. Nothing else is done by hand once the App exists.
- The bot never pushes to `main`; branch protection is unchanged. Only the publish job writes, and only tags and releases.
- Release notes are as good as the `## Unreleased` entries pull requests write; an empty section stops a release.
- The App is a one-time setup by the maintainer. The monthly `flake.lock` update uses it too, so CI, including the release-script tests that guard a cargo-release upgrade, runs on those PRs without manual steps.
