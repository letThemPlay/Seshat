# Continuous integration

Every pull request into `main` runs the full test suite and the security scanners. Pushes to `main` and the merge queue run the same checks.

## Workflows

| Workflow | Job | What it does |
|---|---|---|
| `ci.yml` | **Checks** | `cargo fmt --check`, `clippy -D warnings`, `scripts/check-architecture.sh`, `cargo test --workspace` (including D-Bus tests on a private bus), `scripts/verify-wayland.sh` (headless Weston) |
| | **UI end-to-end** | Three sandboxed headless Sway sessions: `scripts/ui/file-operations.sh`, `scripts/ui/workspace.sh` and `scripts/ui/fuzzy-find.sh`. Screenshots and logs of each are uploaded as artifacts (`ui-e2e-artifacts`, `ui-e2e-workspace-artifacts`, `ui-e2e-fuzzy-artifacts`) when it fails |
| | **Nix package** | `nix build .#seshat` (runs the suite again in the Nix sandbox), Wayland verification of the packaged binary, desktop entry and AppStream validation |
| | **Dependencies (cargo-deny)** | RustSec advisories, the license allow-list, banned and wildcard dependencies, crates.io-only sources (`deny.toml`) |
| `security.yml` | **CodeQL (rust)** / **CodeQL (actions)** | Code scanning of the Rust code (no build needed) and of the workflow files; results go to the Security tab |
| | **Dependency review** | Fails PRs that add dependencies with known vulnerabilities (moderate or worse) |
| | **Workflow security (zizmor)** | Static analysis of the workflows (pinning, injection, permissions); results go to the Security tab |
| `update-flake-lock.yml` | monthly | Opens a PR updating `flake.lock` (Dependabot does not handle Nix) |
| `dependabot.yml` | weekly | Grouped updates for Cargo dependencies and the SHA-pinned actions |

`security.yml` also runs weekly, so newly published advisories surface without a code change.

## Why everything runs through Nix

GitHub's Ubuntu 24.04 runners ship GTK 4.14, and Seshat needs at least 4.20. Every job uses the flake's dev shell or package, so CI uses exactly the toolchain of local development. Nix dependencies come from cache.nixos.org; only Cargo's `target/` is cached (`Swatinem/rust-cache`). The shared setup is in `.github/actions/setup`.

The UI job runs `sysctl kernel.apparmor_restrict_unprivileged_userns=0` first. Ubuntu 24.04 restricts the unprivileged user namespaces that `bubblewrap` (the UI test sandbox) needs.

## Code scanning: what is included and why

| Tool | Status | Reason |
|---|---|---|
| CodeQL (Rust, Actions) | included | Rust is generally available and needs no build; it also checks the workflows |
| cargo-deny | included | advisories, licenses and sources in one exact policy |
| Dependency review | included | diff-based vulnerability gate on PRs. License checking is left to cargo-deny, because GitHub reports many crates' licenses as unknown |
| zizmor | included | security of the CI itself |
| Dependabot | included | Cargo and Actions updates, with a 7-day cooldown |
| Secret scanning and push protection | repository setting | free on public repositories; no workflow needed |
| clippy SARIF | not used | `clippy -D warnings` already fails the PR |
| cargo-audit, OSV-Scanner | not used | covered by cargo-deny and Dependency review |
| Semgrep | not used | its Rust rules are thin; CodeQL covers more |
| cargo-geiger | not used | `unsafe_code = "forbid"` is a workspace lint |

## Workflow hardening

- Every third-party action is pinned to a full commit SHA, with a version comment; Dependabot keeps them current.
- The local action is referenced as `$/.github/actions/setup`, GitHub's self-repository syntax, which can't be swapped for files placed on disk during the job. actionlint 1.7.12 doesn't know this syntax yet; zizmor recommends it.
- Workflows default to `permissions: {}` or read-only, and each job declares only what it needs, with a comment.
- Checkouts use `persist-credentials: false`. There is no `pull_request_target`, and no untrusted `${{ }}` interpolation in shell steps.
- Every job has a timeout, and superseded runs are cancelled.
- `zizmor --persona pedantic .github/` reports no findings.

## Reproducing a job locally

Run these inside `nix develop`:

```sh
scripts/check.sh --ui                                  # Checks + UI end-to-end
nix build .#seshat && SESHAT_BIN=./result/bin/seshat scripts/verify-wayland.sh
nix shell nixpkgs#cargo-deny -c cargo deny check       # Dependencies
nix shell nixpkgs#zizmor -c zizmor --persona pedantic .github/
```

## Repository settings (done in GitHub, not in workflows)

- **Code security:** enable secret scanning and push protection, Dependabot alerts and Dependabot security updates.
- **A ruleset or branch protection on `main`** that requires: `Checks`, `UI end-to-end`, `Nix package`, `Dependencies (cargo-deny)`, `CodeQL (rust)`, `CodeQL (actions)`, `Dependency review` and `Workflow security (zizmor)`.
- **Optional:** a merge queue. `ci.yml` already handles `merge_group`.
- **`update-flake-lock` PRs** are created with `GITHUB_TOKEN`, which doesn't trigger other workflows. Start CI on them with "Run workflow", or give the workflow a GitHub App token.
