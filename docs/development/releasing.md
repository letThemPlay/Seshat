# Releasing

Seshat's releases are numbered with [Semantic Versioning](https://semver.org). Stable releases are `X.Y.Z`; pre-releases add `-alpha.N`, `-beta.N` or `-rc.N` (`0.2.0-alpha.1 → 0.2.0-alpha.2 → 0.2.0-beta.1 → 0.2.0-rc.1 → 0.2.0`). All crates share one version, `[workspace.package] version` in `Cargo.toml`; the Nix package reads it from there, and `seshat --version` prints it.

## The changelog

`CHANGELOG.md` is written by hand. Every pull request that changes something for users adds a few lines under `## Unreleased`, grouped under `###` headings by area. A release turns that section into the release's own section, `## VERSION (DATE)`, and starts a new, empty `## Unreleased`. Pre-releases get their own sections too.

## What a release changes

`scripts/release/prepare.sh LEVEL|VERSION` makes every change a release needs in the working tree, and nothing else:

| File | Change |
|---|---|
| `Cargo.toml`, `Cargo.lock` | the shared workspace version, bumped by [cargo-release](https://github.com/crate-ci/cargo-release) |
| `CHANGELOG.md` | `## Unreleased` becomes `## VERSION (DATE)` under a new `## Unreleased` |
| `data/dev.kdmorris.Seshat.metainfo.xml` | a new `<release>`, newest first; `type="development"` for a pre-release |

It commits, tags and pushes nothing. The level is one of:

| Level | From `0.2.0` | From `0.2.0-alpha.1` |
|---|---|---|
| `alpha`, `beta`, `rc` | `0.2.1-alpha.1` (and so on) | `0.2.0-alpha.2`, `0.2.0-beta.1`, `0.2.0-rc.1` |
| `patch` | `0.2.1` | `0.2.0` |
| `minor` | `0.3.0` | `0.3.0` |
| `major` | `1.0.0` | `1.0.0` |
| `release` | — | `0.2.0` |
| `auto` | from the commits since the last tag (below) | the same label again: `0.2.0-alpha.2` |
| an explicit version | any higher `X.Y.Z` or `X.Y.Z-alpha/beta/rc.N`, for example `0.3.0-alpha.1` to start the next minor version's pre-releases | |

`auto` (`scripts/release/level.sh`) reads the [Conventional Commits](commits.md) since the last `v*` tag: a breaking change (`!` or `BREAKING CHANGE:`) gives `minor` below 1.0 and `major` from 1.0, a `feat` gives `minor`, and anything else `patch`. During a pre-release it repeats the label, so leaving a pre-release is always a deliberate choice.

`prepare.sh` refuses, changing nothing, when:
- `## Unreleased` is empty: describe the changes first;
- the result isn't higher than the current version, is already tagged or already in the changelog;
- a version isn't `X.Y.Z` or `X.Y.Z-alpha/beta/rc.N`;
- the dev shell's cargo-release isn't the series the scripts were tested with (below).

## Checks

- `scripts/release/check-versions.sh` (CI, `scripts/check.sh`): the version in `Cargo.toml`, the newest release in the changelog and the newest `<release>` in the AppStream metadata must agree, and the changelog must have one `## Unreleased`.
- `scripts/release/test.sh` (CI, `scripts/check.sh`): runs `prepare.sh` on a throwaway copy of the repository through a whole cycle (alpha, beta, rc, release, patch, minor, explicit versions, `auto`) and every refusal, checking the exact versions and files.

## Keeping cargo-release from changing under us

cargo-release only bumps the version; everything else is our own script. It comes only from the dev shell, so its version is pinned by `flake.lock`, locally and in CI alike. `CARGO_RELEASE_SERIES` in `scripts/release/common.sh` records the series the scripts are tested with (now 1.1): with any other, `prepare.sh` stops. When the monthly `flake.lock` update brings a new cargo-release, CI runs `test.sh` on it; if it passes, update `CARGO_RELEASE_SERIES` in that pull request.

## Releasing by hand

Until the release workflows exist:

```sh
nix develop
scripts/release/prepare.sh 0.2.0-alpha.1      # or a level
git switch -c release/v0.2.0-alpha.1
git commit -am "chore(release): 0.2.0-alpha.1"
```

Open a pull request; once it is merged, tag the merge as `v0.2.0-alpha.1` and create a GitHub Release from the changelog section, marked as a pre-release.
