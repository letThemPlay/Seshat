# Changelog

Written by hand: every pull request describes what changes for users under **Unreleased**. A release turns that section into the release's own section ([releasing.md](docs/development/releasing.md)); versions follow [Semantic Versioning](https://semver.org), with `-alpha.N`, `-beta.N` and `-rc.N` pre-releases.

## Unreleased

## 0.1.1-alpha.1 (2026-11-15)

### Home Manager, Stylix and development

- **Release workflows:** **Actions → Prepare release** opens a release pull request, `chore(release): VERSION`, with the version bumped and the release notes from this changelog; merging it tags `vVERSION` and publishes the GitHub Release, as a pre-release for alpha, beta and rc versions ([releasing.md](docs/development/releasing.md), [ADR 0010](docs/decisions/0010-releases.md)). The PR is opened by a GitHub App, so CI runs on it; the monthly `flake.lock` update uses the App too once it is set up.
- **`seshat --version`** (or `-V`) prints the version, for example `seshat 0.2.0-alpha.1`. It is the workspace version a release sets, so it always matches the release's tag and changelog section.
- **Release tooling:** `scripts/release/prepare.sh` prepares a release, alpha, beta and rc pre-releases included: it bumps the version with cargo-release and turns this changelog's Unreleased section and the AppStream metadata into the release's entries ([releasing.md](docs/development/releasing.md), decisions R1–R4). CI now checks that `Cargo.toml`, the changelog and the AppStream metadata agree on the version, and runs a whole release cycle on a throwaway copy. The changelog's three Unreleased sections are now one, and 0.1.0 is dated 2026-11-07 as in the AppStream metadata.
- **Home Manager and Stylix guide:** [docs/home-manager](docs/home-manager/README.md) explains the module and the Stylix integration, with complete standalone Home Manager and NixOS examples. The test flake evaluates the examples exactly as they are and checks that the guide shows them verbatim. [ADR 0009](docs/decisions/0009-home-manager-stylix.md) records the design. A new UI script, `scripts/ui/stylix-themes.sh`, screenshots Seshat with the themes generated for the specification's five visual scenarios ([results](docs/development/verification-results/2026-11-14-stylix.md), awaiting the maintainer's review).
- **Readable muted text from Stylix:** muted text (file types, dates, the status bar) now uses the scheme's `base04` instead of `base03`, Base16's faint comment colour, and secondary text (navigation labels, panel titles, the path bar) uses `base05` so it is never fainter than muted text (decision S9). To get dimmer secondary text back, set `programs.seshat.theme.colors.text_secondary` to `base04`.
- **Stylix:** with Stylix imported and enabled, the Home Manager module themes Seshat from your Stylix colours, sans-serif font, application font size and polarity, and Seshat starts with that theme ([packaging.md](docs/packaging.md#stylix), decisions S8–S10). It is on when `stylix.autoEnable` is true. `programs.seshat.stylix.{colors,fonts,polarity}.enable` turn parts off, and your own `programs.seshat.theme` values win over Stylix's one by one. Enabling it without Stylix fails with an explanation. Stylix is only read while Home Manager evaluates the configuration; Seshat doesn't depend on it. Every background under the main text is the scheme's base00, so 282 of the 303 Base16 schemes in nixpkgs give a theme with no problems; the other 21 get Seshat's usual low-contrast warning, because their own text and background are below 4.5:1. The test flake now has Stylix as an input and 17 checks, including NixOS with Stylix's NixOS module.
- **Selected navigation item colour:** themes can give the selected navigation item its own background with the new `navigation_selected` colour (also in the Home Manager module's `theme`). Leave it out and the selected item keeps following `surface`, exactly as before, so existing themes look the same. The 12px corners stay fixed. This changes a requirement of the original specification (§6.3, §7.3, §7.4) and the Stylix specification (§6), which were amended (decision T1).
- **Home Manager module:** the flake now exports `homeModules.default` (also `homeManagerModules.default`) with `programs.seshat`. `enable` installs Seshat, and the optional `theme` takes the keys of a theme file, checked when the configuration is evaluated. With a theme, the installed `seshat` is a wrapper that always starts with it (`--theme-file`), and `settings.toml` is left alone. It works in standalone Home Manager and in Home Manager inside NixOS. See [packaging.md](docs/packaging.md#home-manager-module). Seven checks in `nix/tests`, a separate flake with Home Manager as its only extra input, run in the **Nix package** CI job together with `nix flake check`.
- **Theme given at launch:** `seshat --theme-file FILE` starts with the theme in `FILE`, a normal theme file, which the coming Home Manager module will use for Stylix ([specification](docs/specifications/5-STYLIX-INTEGRATION.md)). It is listed first in the theme menu under **Launch Theme** and wins over your saved theme at every launch. You can still pick another theme for the session, but the launch theme itself is never saved, so starting Seshat without the option behaves as before. A file that can't be read or parsed is reported, and the saved theme is used.
- **Checking themes:** `seshat --check-theme FILE` loads a theme exactly as Seshat would, without opening a window, prints every problem and exits with status 0 only if there are none.
- **Conventional Commits:** commit messages and pull request titles now follow [Conventional Commits](docs/development/commits.md) (`fix(workspace): …`, `feat(fuzzy-find): …`). A new CI job, **Commit messages**, checks every commit of a pull request with `scripts/check-commits.sh`, which `scripts/check.sh` also runs locally. Dependabot and the flake update write messages in the same format. Earlier history is unchanged.

### Fuzzy Find

Implements the [Fuzzy Find specification](docs/specifications/4-FUZZY-FIND.md), except operations on several results at once (§10), which wait for a later phase (F13). Design: [search.md](docs/architecture/search.md); decisions F1–F14 in [DECISIONS.md](docs/architecture/DECISIONS.md); opening results: [ADR 0008](docs/decisions/0008-opening-search-results.md).

- **Fuzzy Find (`Ctrl+P`):** a compact overlay over the workspace that finds files and folders as you type, fzf-style (`prj` finds `Projects`). Matched characters are bold and underlined; folders have an icon and a trailing `/`. `Up`, `Down`, `Page Up` and `Page Down` move the highlight, `Enter` opens, `Esc` or clicking elsewhere closes. The workspace doesn't change until you open something. It isn't available in Visual Mode, and the status bar says why. The shortcut is the `fuzzy-find` binding; your own binding for `Ctrl+P` wins.
- **Scopes:** Current Directory, Current Branch or Entire Workspace. `Tab` and `Shift+Tab` cycle through them, `Alt+1`, `Alt+2`, `Alt+3` jump to one, and all three are shown as buttons with their numbers; each change is announced to screen readers. With Entire Workspace, each result shows the number of the branch it was found in, and folders shared by several branches are searched once. The scope you pick lasts until Seshat closes.
- **Opening results:** a folder opens one panel per level from the panel that was searched, reusing panels already open, bringing back a branch that already has it, and starting at most one new branch. One Back returns to where you were. A file opens in its default application and the workspace stays as it was.
- **Reveal (`Alt+Enter`):** opens the folder a result is in and puts the cursor on it, without changing that folder's selection.
- **Typed folders:** a search starting with `/`, `~/` or `~` looks inside that folder (`~/Projects/main` finds `main` in `~/Projects`). With nothing after the folder, `Enter` opens it, as the address bar would. A folder that doesn't exist is reported, not opened.
- **Settings:** a new `[fuzzy_find]` section in `settings.toml` with `default_scope`, `include_hidden`, `ignore` (exact names; `.git` and `node_modules` by default), `max_results` (500) and `case` (`insensitive`, `smart` or `sensitive`). See [docs/examples/settings.toml](docs/examples/settings.toml).
- **Fast and safe:** searches run on a background thread, and each new query cancels the previous one. The folders are walked once and reused for every query while the overlay is open. In a release build a 200-file folder shows results in about 2 ms, and 100,100 entries are walked in about 31 ms and ranked in 24–42 ms, with no pauses in the window ([measurements](docs/architecture/search.md#with-the-overlay-release-build-2026-11-12)). Symbolic links are never followed, unreadable folders are skipped and counted, and files are never changed.
- **Status messages:** every status-bar message is now a sentence, errors are drawn in the theme's error colour, and errors are announced to screen readers (decisions N1–N3).
- **Internal:** the design review's recommendations DR1–DR10 were carried out with no change in behaviour ([review](docs/review/architecture.md#patterns-and-principles-review-2026-11-11)). Fuzzy Find's search rules (what a query searches, when Enter opens a typed folder, when results name their branch) and its highlight now live in the application layer (`SearchTarget`, `ResultList`) instead of the overlay's controller, and repeated code in Fuzzy Find was merged, also with no change in behaviour.
- **Tests:** 451 Rust tests and a third sandboxed UI test, `scripts/ui/fuzzy-find.sh` (38 checks). `scripts/ui/fuzzy-perf.sh` measures Fuzzy Find with the overlay in a release build.

### Spatial workspace

- **Backward branch pruning** ([ADR 0007](docs/decisions/0007-backward-branch-pruning.md)): `db` removes the active panel from the end of its branch and goes back to its parent; `dB` goes back to where the branch meets another one (or to its first panel). Only panels no other branch uses are removed, and files are never touched. `u` undoes the last prune with every panel's state. Neither works in Focus Mode or Visual Mode; the status bar says why. Your own bindings for `d` or `u` keep working: the new defaults give way.
- **Context-aware navigation** ([ADR 0006](docs/decisions/0006-context-aware-navigation.md)): `l`, like `Enter`, opens the folder under the cursor, and Seshat decides whether that continues the current branch, returns to the branch that already has that folder (where you left it, with its panels and state), or starts a new branch. Nothing is duplicated. The plain move to the next panel is now `L`. Folders renamed in place are followed; a folder replaced under the same name starts fresh. Sidebar locations reuse their existing branch. The saved workspace is format 2; older files still load. `/` now replaces a panel's remembered search instead of appending to it.
- **UI fixes:** every panel header now has the same height, whether or not it shows a branch badge, a fork button or Focus Mode arrows. Moving to a panel scrolls it fully into view, including the rightmost panel and panels that widen as they become active, and the view no longer drifts when keyboard focus moves or the layout changes.
- **Software centre metadata:** the developer (Kelvin Morris) is named, and the description now covers the spatial workspace.
- **Example theme:** [Catppuccin Mocha](docs/themes/catppuccin-mocha.toml), installed with the package as `share/doc/seshat/examples/themes/catppuccin-mocha.toml`.
- **Licence:** GPL-3.0-or-later confirmed; the full text is in `LICENSE` and installed with the package (`share/licenses/seshat`).
- **Performance run** ([results](docs/review/performance.md)): all approved targets met at 50 branches and 211 panels. Saving the workspace no longer blocks the UI (the write and its `fsync` run on a writer thread). Opt-in benchmarks and `scripts/ui/workspace-perf.sh` for reruns.
- **Command-line completion:** Tab completes command names, folder paths for `:copy`, `:move` and `:open` (with `~`, relative paths and `..`), and branch targets such as `b2`. Repeated Tab and Shift+Tab cycle through several matches, which are listed in the status bar.
- **Branches side by side** ([ADR 0005](docs/decisions/0005-branch-layout.md)): tree depth across, branches down. Shared folders span their branches; connectors link folders to subfolders, with the active path in the accent colour. Other branches are compact cards. Short windows fall back to one row with "+N branches" stubs; `layout = "strip"` keeps the single strip.
Implements the [spatial file manager specification](docs/specifications/1-SPATIAL-FILE-MANAGER.md) on top of 0.1.0. Product decisions D1–D19 are in [DECISIONS.md](docs/architecture/DECISIONS.md) and the architecture in [ADR 0004](docs/decisions/0004-spatial-workspace.md).

- **Workspace:** folders open as persistent panels in a horizontally scrolling strip. Each panel keeps its cursor, scroll position, sort order, view, search and selection.
- **Branches:** opening another folder from a panel that already has a child starts a new branch; shared folders exist once. Switch with `[` and `]`, the branch bar or `:branch`; close branches without touching files.
- **Focus Mode** (`zz` or the toolbar button) expands the active panel; navigation and branch switching keep working.
- **Vim-style keys:** Normal, Visual (`v`, `V`) and Command (`:`) modes. Bindings are configurable in `settings.toml`, and `:help` lists them.
- **Operations across panels and branches:** the workspace-wide selection is copied or moved to an explicit destination (paste into a panel, `:copy`/`:move`, the destination picker, or drag and drop), with a confirmation when the sources come from several panels. The operation manager and the D-Bus protocol are unchanged.
- **The workspace is saved** to `$XDG_STATE_HOME/seshat/workspace.toml` (versioned) and restored on launch.
- **Behaviour changes:**
  - Typing letters no longer starts a search; use `/` or Ctrl+F, or set `classic_keys = true` (D1).
  - Back and Forward return to previously active panels (D2).
  - Sidebar and address-bar locations open as new branches (D3).
  - A deleted folder or removed device leaves its panels in place, marked unavailable, instead of returning Home (D14).
  - Selections never include items hidden by a search filter.
- **Tests:** 295 Rust tests (from 165) and a second sandboxed UI test, `scripts/ui/workspace.sh`, which walks the spec's definition of done.

## 0.1.0 (2026-11-07)

First complete version, built in six phases (see `docs/specifications/0-INITIAL-PRODUCT-SPEC.md` §17).

- **Foundation:** GTK4 + Relm4 ([ADR 0001](docs/decisions/0001-ui-framework.md)), Clean Architecture workspace, native Wayland verification.
- **Filesystem:** XDG locations, async GIO listing, navigation history, breadcrumbs, address bar, live updates, Trash browsing, removable and network devices.
- **UI and theming:**
  - list and grid views, natural sorting, search, multi-selection, context menus, drag-out
  - light, dark and validated custom TOML themes
  - preferences saved between sessions
- **File operations:**
  - a central Operation Manager handling copy, move, rename, delete, Trash, restore and create
  - progress, cancellation, conflict resolution and partial-failure reporting
  - atomic overwrites; work continues after the window closes
  - clipboard interoperability with GNOME and KDE file managers, and drop-in
- **D-Bus IPC:** a typed, versioned protocol with sequence numbers and snapshots; the independent example client `seshat-progress` ([ADR 0002](docs/decisions/0002-ipc.md)).
- **Release work:**
  - a Nix package with desktop entry, icon and AppStream metadata ([ADR 0003](docs/decisions/0003-packaging.md))
  - security, performance and architecture reviews with fixes ([docs/review](docs/review))
  - automated architecture checks and a single `scripts/check.sh`
