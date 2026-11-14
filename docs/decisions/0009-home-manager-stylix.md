# ADR 0009: Home Manager module and Stylix theming

- **Status:** Accepted
- **Date:** 2026-11-14
- **Spec references:** [Stylix integration](../specifications/5-STYLIX-INTEGRATION.md); the theme architecture of the [original specification](../specifications/0-INITIAL-PRODUCT-SPEC.md) §6.3 and §7; decisions S1–S10 and T1 in [DECISIONS.md](../architecture/DECISIONS.md); builds on [ADR 0003](0003-packaging.md)

## Context

Seshat's themes are TOML files in `~/.config/seshat/themes/`, layered on a built-in light or dark base and validated by the presentation layer's parser. The chosen theme is saved in `settings.toml`, which Seshat rewrites whenever a setting changes. The flake shipped a package but no module.

The Stylix specification asks for an optional Home Manager module that themes Seshat from Stylix, reuses the existing theme system, adds no runtime dependency on Stylix, works for standalone Home Manager and NixOS with Home Manager, and lets users override values one by one.

Two facts shaped the design:
- **`settings.toml` can't be managed by Home Manager.** Seshat replaces it atomically on every save, which would replace Home Manager's symlink and make the next switch fail.
- **The theme choice lives in that file.** A theme written to the themes folder would only be offered, not applied.

## Decision

1. **A launch theme (S1, S2).** `seshat --theme-file FILE` starts Seshat with the theme in `FILE`, read by `FileThemeRepository` and parsed by the same code as the themes folder. It is `ThemeChoice::Provided`: shown first in the theme menu, applied at every launch, never saved. `seshat --check-theme FILE` loads a theme the same way without a display and fails on any problem (S4).
2. **A Home Manager module that wraps the package (S1, S3, S5).** `homeModules.default` provides `programs.seshat` with `enable`, `package` and `theme`, whose typed options mirror the theme file. With a theme, the installed `seshat` is a `symlinkJoin` of the flake's package with a binary wrapper that adds `--theme-file <generated file>`. The package derivation itself is unchanged, and `settings.toml` is not touched.
3. **Stylix fills `programs.seshat.theme` (S6, S8–S10).** With Stylix imported and enabled (and `stylix.autoEnable`, like Stylix's own targets), `nix/stylix.nix` sets the theme from `config.lib.stylix.colors`, `stylix.fonts` and `stylix.polarity`, every value with `mkDefault`, so the user's own definitions win one by one. Base16 is mapped onto Seshat's existing tokens with base00 behind all text. The module never imports Stylix and fails with an assertion if enabled without it.
4. **The selected navigation item's colour becomes a token (T1)**, at the maintainer's request: `navigation_selected`, following `surface` when unset. The original specification §6.3, §7.3 and §7.4 and the Stylix specification §6 were amended.
5. **Tests in a separate flake (S7).** `nix/tests` has Home Manager and Stylix as inputs, so the main flake keeps only nixpkgs and flake-utils. Its checks evaluate the module in each scenario, evaluate the guide's examples as they are, and have the wrapped Seshat check every generated theme.

## Alternatives considered

- **Write `themes/stylix.toml` and let the user pick it once** (the first plan). No Rust change, but not automatic, and switching Stylix off would leave a dangling choice. Replaced by the launch theme at the maintainer's request.
- **Manage `settings.toml`, or edit it at activation.** Clobbers or fights the app's own saves.
- **An environment variable instead of a flag.** Desktop launchers don't reliably see Home Manager's session variables, and the specification rules out depending on them.
- **A NixOS module.** Theming is per user; Stylix's NixOS module already imports Home Manager modules for every user.
- **Options under `stylix.targets.seshat`.** Couples Seshat to Stylix's internal target helpers and could clash with a future target inside Stylix.

## Consequences

- Nix users get Seshat themed from Stylix with `programs.seshat.enable = true`; everyone else is unaffected, and Seshat runs anywhere with the generated file alone.
- There is one theme engine: Nix only writes a theme file, and Seshat validates it as any other.
- A new theme shows after Seshat restarts, not live.
- Contrast follows the scheme: 282 of 303 Base16 schemes in nixpkgs give a theme without warnings. Base16 has only two text colours, so muted text is base04 and secondary text shares base05 with the main text. The header bar keeps the desktop's GTK look (see [verification](../development/verification-results/2026-11-14-stylix.md)).
- `nix/tests/flake.lock` is not updated by the monthly lock job yet.
