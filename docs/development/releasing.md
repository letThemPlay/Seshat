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
- `scripts/release/test.sh` (CI, `scripts/check.sh`): runs `prepare.sh` on a throwaway copy of the repository, reset to a fixed starting point (0.1.0, released once) so the result doesn't depend on the real version, through a whole cycle (alpha, beta, rc, release, patch, minor, explicit versions, `auto`) and every refusal, checking the exact versions and files. It also checks what publishing does (`to-publish.sh`, `notes.sh`).

## Keeping cargo-release from changing under us

cargo-release only bumps the version; everything else is our own script. It comes only from the dev shell, so its version is pinned by `flake.lock`, locally and in CI alike. `CARGO_RELEASE_SERIES` in `scripts/release/common.sh` records the series the scripts are tested with (now 1.1): with any other, `prepare.sh` stops. When the monthly `flake.lock` update brings a new cargo-release, CI runs `test.sh` on it; if it passes, update `CARGO_RELEASE_SERIES` in that pull request.

## Making a release

1. Check that `## Unreleased` in `CHANGELOG.md` on `main` says everything the release should say.
2. **Actions → Prepare release → Run workflow.** Pick a **level** (`auto` by default), or type an exact **version** such as `0.3.0-alpha.1`. Tick **dry-run** to see the changes in the job log without opening a PR.
3. The release App opens **`chore(release): VERSION`** from `release/vVERSION`, with the release notes as its description. CI runs on it like on any PR. Review the diff: `Cargo.toml`, `Cargo.lock`, the changelog and the AppStream metadata. To change the notes, edit `CHANGELOG.md` in that branch.
4. **Merge it.** "Publish release" tags the merged commit `vVERSION` and publishes the GitHub Release named "Seshat VERSION" with the changelog section. Alpha, beta and rc versions are marked as pre-releases and never become "Latest".

Users can then pin it, for example `github:letThemPlay/Seshat/v0.2.0-alpha.1`.

"Publish release" runs on every push to `main` and only acts when the push changed the workspace version and that version has no tag yet (`scripts/release/to-publish.sh`). Other pushes, re-runs and an already-tagged version publish nothing.

## The release App (one-time setup)

The release PR is opened by a GitHub App, because pull requests opened with the workflow's own `GITHUB_TOKEN` don't start CI, so their required checks would never run. A personal access token would act as you, and you couldn't approve your own release PR. The App's tokens are created per run and expire after an hour.

1. **GitHub → Settings → Developer settings → GitHub Apps → New GitHub App.**
   - **Name:** for example `seshat-release`. **Homepage URL:** the repository's URL.
   - **Webhook:** untick **Active**.
   - **Repository permissions:** **Contents: Read and write**, **Pull requests: Read and write** (Metadata: Read-only is added automatically). Nothing else.
   - **Where can this GitHub App be installed?** Only on this account.
2. **Create GitHub App.** On its page, note the **Client ID**, then **Generate a private key**; a `.pem` file downloads.
3. **Install App** (left menu) → your account → **Only select repositories** → `Seshat`.
4. In the repository: **Settings → Secrets and variables → Actions**:
   - **Variables → New repository variable:** `RELEASE_APP_CLIENT_ID` = the Client ID.
   - **Secrets → New repository secret:** `RELEASE_APP_PRIVATE_KEY` = the whole contents of the `.pem` file. Then delete the downloaded file.
5. If a ruleset requires approvals, the App's PRs need your approval like any other.

The monthly `flake.lock` update uses the same App once it is set up, so CI runs on its PRs by itself. Without the App it falls back to `GITHUB_TOKEN`, as before.

## Releasing by hand

If the workflows can't be used:

```sh
nix develop
scripts/release/prepare.sh 0.2.0-alpha.1      # or a level
git switch -c release/v0.2.0-alpha.1
git commit -am "chore(release): 0.2.0-alpha.1"
```

Open a pull request and merge it; "Publish release" then tags and publishes as usual.
