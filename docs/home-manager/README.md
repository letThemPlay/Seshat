# Seshat with Home Manager and Stylix

Seshat's flake has a Home Manager module, `programs.seshat`. It installs Seshat and can give it a theme declaratively. If you use [Stylix](https://github.com/nix-community/stylix), it themes Seshat from your Stylix colours and fonts automatically, so you don't configure the same values twice ([specification](../specifications/5-STYLIX-INTEGRATION.md), decisions S1–S10 and T1 in [DECISIONS.md](../architecture/DECISIONS.md#stylix-and-home-manager-integration-plan-approved-2026-11-14)).

## What it does

- Installs Seshat (the package from Seshat's flake).
- Turns `programs.seshat.theme`, or your Stylix palette, fonts and polarity, into an ordinary [Seshat theme file](../development/themes.md).
- Installs `seshat` as a small wrapper that always starts Seshat with that theme (`seshat --theme-file <file>`).

Seshat shows the theme first in its theme menu, under **Launch Theme**, and uses it every time it starts. You can still pick another theme for a session; that choice is saved as usual, but the launch theme is never saved, so starting Seshat some other way behaves as before.

Stylix is only read while Home Manager evaluates your configuration. The result is a plain theme file: Seshat never runs, reads or links anything from Stylix, and needs no Stylix to run. The module never touches `~/.config/seshat/settings.toml`, which Seshat rewrites itself whenever a setting changes.

## Supported environments

| Setup | How Seshat's module is imported |
|---|---|
| Standalone Home Manager | in `homeManagerConfiguration`'s `modules` |
| NixOS with Home Manager | in `home-manager.sharedModules` (or one user's `imports`) |

Both use the same module; there is no NixOS module, because Seshat's theme is per user. Seshat's flake supports `x86_64-linux` and `aarch64-linux`. Stylix is optional: without it, the module installs Seshat and applies `programs.seshat.theme` if you set one.

## Installation

Add Seshat's flake as an input, next to Home Manager and, if you use it, Stylix. The module is exported as `homeModules.default` (also `homeManagerModules.default`, the older name).

### Standalone Home Manager

`flake.nix`:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    stylix = {
      url = "github:nix-community/stylix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    seshat.url = "github:letThemPlay/Seshat";
  };

  outputs =
    {
      nixpkgs,
      home-manager,
      stylix,
      seshat,
      ...
    }:
    {
      homeConfigurations.alice = home-manager.lib.homeManagerConfiguration {
        pkgs = nixpkgs.legacyPackages.x86_64-linux;
        modules = [
          stylix.homeModules.stylix
          seshat.homeModules.default
          ./home.nix
        ];
      };
    };
}
```

`home.nix`:

```nix
{ config, pkgs, ... }:
{
  home.username = "alice";
  home.homeDirectory = "/home/alice";
  home.stateVersion = "25.05";

  stylix = {
    enable = true;
    base16Scheme = "${pkgs.base16-schemes}/share/themes/gruvbox-dark-hard.yaml";
    polarity = "dark";
  };

  programs.seshat = {
    enable = true;
    # Stylix fills in the theme. Your own values win, one by one:
    theme.colors.text_secondary = config.lib.stylix.colors.withHashtag.base04;
  };
}
```

Then run `home-manager switch --flake .#alice`.

### NixOS with Home Manager

Stylix's NixOS module themes every user through Home Manager, so the user's own configuration only enables Seshat.

`flake.nix`:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    stylix = {
      url = "github:nix-community/stylix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    seshat.url = "github:letThemPlay/Seshat";
  };

  outputs =
    {
      nixpkgs,
      home-manager,
      stylix,
      seshat,
      ...
    }:
    {
      nixosConfigurations.desktop = nixpkgs.lib.nixosSystem {
        modules = [
          ./configuration.nix
          home-manager.nixosModules.home-manager
          stylix.nixosModules.stylix
          {
            home-manager.sharedModules = [ seshat.homeModules.default ];
            home-manager.users.alice = ./home.nix;
          }
        ];
      };
    };
}
```

`configuration.nix` (with the rest of your system configuration):

```nix
{ pkgs, ... }:
{
  # Stylix's NixOS module themes the system and, through Home Manager,
  # every user, including Seshat.
  stylix = {
    enable = true;
    base16Scheme = "${pkgs.base16-schemes}/share/themes/gruvbox-light-hard.yaml";
    polarity = "light";
  };

  users.users.alice.isNormalUser = true;
  system.stateVersion = "25.05";
}
```

`home.nix`:

```nix
{
  home.stateVersion = "25.05";

  # Themed from the system's Stylix settings; nothing else is needed.
  programs.seshat.enable = true;
}
```

Then run `nixos-rebuild switch --flake .#desktop`.

These files are in [`examples/`](examples). The checks in `nix/tests` evaluate them exactly as they are, build the wrapped Seshat and have it check the generated theme (`nix flake check ./nix/tests`).

## Enabling

| Option | Default | Meaning |
|---|---|---|
| `programs.seshat.enable` | `false` | Install Seshat |
| `programs.seshat.package` | Seshat's flake package | `null` installs nothing; then no theme can be applied, because the theme is passed by the wrapper |
| `programs.seshat.theme` | `null` | A theme Seshat always starts with (below) |
| `programs.seshat.stylix.enable` | `true` when Stylix is imported, `stylix.enable` and `stylix.autoEnable` are true | Fill `programs.seshat.theme` from Stylix |
| `programs.seshat.stylix.colors.enable` | `true` | Use Stylix's colours |
| `programs.seshat.stylix.fonts.enable` | `true` | Use Stylix's sans-serif font and application font size |
| `programs.seshat.stylix.polarity.enable` | `true` | Use Stylix's polarity as the base theme |
| `programs.seshat.themeFile`, `finalPackage` | read-only | The generated theme file, and the package that is installed |

Like Stylix's own targets, the Stylix part is on whenever Stylix is enabled and `stylix.autoEnable` is true; you only need `programs.seshat.enable = true`. Seshat's module never imports or installs Stylix or its fonts: import Stylix yourself. If you set `programs.seshat.stylix.enable = true` without Stylix, the configuration fails with a message saying so.

## Colour mapping

Stylix's Base16 colours become Seshat's colour tokens (decision S9):

| Seshat token | Used for | Stylix |
|---|---|---|
| `surface` | Content pane, workspace panels | `base00` |
| `app_background` | Window, toolbar, status bar | `base00` |
| `navigation_surface` | Navigation pane | `base00` |
| `navigation_selected` | Selected navigation item | not set: follows `surface` |
| `text_primary` | Main text | `base05` |
| `text_secondary` | Navigation labels, panel titles, path bar | `base05` |
| `text_muted` | File types, dates, status bar | `base04` |
| `accent` | Selected-item icon, active panel, checked items | `base0D` |
| `focus` | Keyboard focus outline | `base0D` |
| `border` | Outlines, separators, connectors | `base02` |
| `hover` | Hovered rows and buttons | `base05` at 8% |
| `selected` | Selected files | `base0D` at 30% |
| `error` | Errors | `base08` |
| `warning` | Warnings | `base09` |
| `success` | Success | `base0B` |

Every background behind the main text is `base00`, the scheme's own text background, so the text keeps the contrast the scheme was designed with. Borders separate the areas, and the selected navigation item stands out by its 12px outline and accent icon.

Base16 has two text colours, `base05` and `base04`; `base03` is meant for comments and is too faint for text you need to read (it reaches 3:1 against `base00` in only 115 of the 303 schemes in nixpkgs, `base04` in 267). So muted text is `base04`, and secondary text is `base05` like the main text, so that it is never fainter than muted text. If you prefer dimmer secondary text, set it to `base04`, as the standalone example does.

`base01`, `base03`, `base06`, `base07`, `base0A`, `base0C`, `base0E` and `base0F` are not used. Warnings use orange (`base09`) because yellow is unreadable on light schemes.

## Font mapping

| Seshat | Stylix |
|---|---|
| `typography.font_family` | `stylix.fonts.sansSerif.name` |
| `typography.font_size` | `stylix.fonts.sizes.applications`, rounded to whole points |

Seshat uses one interface font, so Stylix's monospace, serif and emoji fonts are not used; Stylix has no font weight. If the font isn't installed, fontconfig picks a fallback, as for any theme; Stylix installs the fonts you configure. A size outside the 6 to 36 points Seshat accepts is not passed on: Seshat keeps its default size and Home Manager prints a warning.

## Polarity

`stylix.polarity = "dark"` or `"light"` becomes the theme's `extends`, its base theme. With `"either"` (Stylix's default) nothing is set and nothing is guessed from the colours; Seshat's default base is `light`. With Stylix's colours on, the base only fills values the mapping doesn't set, so it hardly shows; with `colors.enable = false` it decides between Seshat's light and dark colours.

Seshat's own **Follow System** theme, which switches between its built-in light and dark themes, is unaffected; it is still in the theme menu.

## Your own values

Every value Stylix fills in can be replaced individually, without turning Stylix off:

```nix
programs.seshat.theme = {
  name = "My Gruvbox";                 # shown in the theme menu
  colors.accent = "#fe8019";           # wins over Stylix's base0D
  colors.text_secondary = config.lib.stylix.colors.withHashtag.base04;
  colors.border = null;                # Seshat's base theme instead of Stylix
  typography.font_size = 11;
  geometry.border_radius = 6;          # Stylix has no geometry; any theme key works
};
```

The order is (decision S8):

1. your own values in `programs.seshat.theme`;
2. Stylix's values (set with `lib.mkDefault`, so ordinary definitions win);
3. Seshat's base theme (`extends`), for anything left unset or set to `null`.

Every key of a [theme file](../development/themes.md) is an option, and values are checked when the configuration is evaluated: a mistyped colour, an unknown key or a number out of range is an evaluation error, not a surprise at runtime. Without Stylix, `programs.seshat.theme` works the same way, with your values on top of the base theme.

## Turning parts off

```nix
programs.seshat.stylix.enable = false;           # no Stylix theme at all
programs.seshat.stylix.colors.enable = false;    # keep Seshat's colours
programs.seshat.stylix.fonts.enable = false;     # keep Seshat's font
programs.seshat.stylix.polarity.enable = false;  # don't set the base theme
```

With `stylix.enable = false` and no `theme` of your own, Seshat is installed unwrapped and uses the theme you pick in the app, exactly as without the module.

## Known limitations

- **Restart to see a new theme.** The theme file is part of the wrapper, so after `home-manager switch` a running Seshat keeps its old theme until it is started again. A second launch while Seshat is running only opens a window in the running instance.
- **Install Seshat only through the module.** Another, unwrapped `seshat` (for example from `environment.systemPackages`) may come first on `PATH` and start without the theme.
- **Contrast follows the scheme.** 282 of the 303 Base16 schemes in nixpkgs give a theme with no problems. The other 21 have main text below 4.5:1 against their own background; Seshat still applies the theme, marks it ⚠ in the menu and says so in the status bar.
- **Muted text follows the scheme's `base04`.** In 36 of the 303 schemes it is below 3:1 against `base00`. Seshat's contrast check only covers the main text, so it doesn't warn; set `programs.seshat.theme.colors.text_muted` if you find it hard to read.
- **The title bar is not themed.** The window's GTK header bar keeps your desktop's GTK look, whatever the Seshat theme.
- **Not every Stylix setting has a place in Seshat:** opacity, cursor, icons, wallpaper, and the colours and fonts listed as unused above.
