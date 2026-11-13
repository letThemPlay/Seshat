# Commit messages

Commit messages and pull request titles follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/), so the kind of change shows at a glance. CI checks every commit of a pull request ([ci.md](ci.md)).

## Format

```
<type>[(<scope>)][!]: <summary>

[body: what and why, wrapped at 72 characters]

[trailers, e.g. Co-Authored-By:]
```

### Type

| Type | For |
|---|---|
| `feat` | something new the user can do or see |
| `fix` | a bug fix |
| `refactor` | a code change with no change in behaviour |
| `perf` | a change that only makes something faster or lighter |
| `docs` | documentation only |
| `test` | tests only |
| `build` | Cargo, Nix, packaging, dependencies |
| `ci` | GitHub workflows, Dependabot, CI scripts |
| `chore` | anything else that changes no shipped behaviour (`.gitignore`, editor files) |
| `style` | formatting only |
| `revert` | undoing an earlier commit |

### Scope

Optional. The area touched, in lowercase kebab-case: for example `workspace`, `fuzzy-find`, `file-ops`, `dbus`, `theme`, `deps`, `nix`. There is no fixed list; leave the scope out when a change spans several areas.

### Breaking changes

A `!` before the colon marks a change that breaks something users or other programs rely on: `settings.toml`, the D-Bus interface or the saved workspace format. Add a `BREAKING CHANGE:` footer that says what breaks and what to do about it.

```
feat(workspace)!: save the workspace as format 3

BREAKING CHANGE: workspaces saved by this version can't be opened by
older ones. Older files still load.
```

### Summary

- Imperative mood ("fit", not "fits" or "fitted").
- Starts with a lowercase letter, and has no trailing full stop.
- The whole subject line is at most 72 characters.
- Plain words that say what changes for the user, as before.

### Exceptions

- Merge commits (GitHub's `Merge pull request #…`) are not checked.
- git's own `Revert "…"` subjects are accepted as they are.

## Examples

Past commits, written in this format:

| Before | After |
|---|---|
| Fit the Focus Mode panel without a horizontal scrollbar | `fix(workspace): fit the Focus Mode panel without a scrollbar` |
| Keep word-by-word smart case in Fuzzy Find | `fix(fuzzy-find): keep word-by-word smart case` |
| Add the Fuzzy Find overlay with the Current Directory scope | `feat(fuzzy-find): add the overlay with the Current Directory scope` |
| Announce error messages to screen readers | `feat: announce error messages to screen readers` |
| Replace boolean flags with the enums they stand for | `refactor: replace boolean flags with the enums they stand for` |
| Extract the workspace jump list into a JumpList type | `refactor(workspace): extract the jump list into a JumpList type` |
| Bring the changelog, README and other docs up to date for Fuzzy Find | `docs: bring the changelog and README up to date for Fuzzy Find` |
| Ignore local Rust, Nix, Claude Code and editor files | `chore: ignore local Rust, Nix, Claude Code and editor files` |

The bots follow the same format: Dependabot writes `build(deps): …` for Cargo and `ci(deps): …` for actions, and the monthly flake update is `build(nix): update flake.lock`.

## Checking

```sh
scripts/check-commits.sh origin/main          # this branch's commits
scripts/check-commits.sh <base> <head>        # any range
```

`scripts/check.sh` runs the first form when `origin/main` exists. To fix a message, reword it (`git commit --amend`, or `git rebase` for older commits) and push with `--force-with-lease`. Only rewrite commits that aren't on `main` yet.
