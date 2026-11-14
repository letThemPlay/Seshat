# Packaging

Seshat currently ships as a **Nix flake package** (see [ADR 0003](decisions/0003-packaging.md)).

## Using the package

```sh
nix run github:letThemPlay/Seshat            # or, in a checkout: nix run .
nix build .#seshat && ./result/bin/seshat
nix profile install .#seshat               # per-user install
nix run .#seshat-progress                  # the example D-Bus client
```

On NixOS, add the flake as an input and put `inputs.seshat.packages.${system}.default` in `environment.systemPackages` (or `home.packages` with Home Manager).

## Home Manager module

The flake exports a Home Manager module, `homeModules.default` (also available as `homeManagerModules.default`). It works the same in standalone Home Manager and in Home Manager inside NixOS.

```nix
# flake.nix
inputs.seshat.url = "github:letThemPlay/Seshat";

# standalone: homeManagerConfiguration { modules = [ inputs.seshat.homeModules.default ./home.nix ]; … }
# NixOS:      home-manager.sharedModules = [ inputs.seshat.homeModules.default ];
```

```nix
# home.nix
programs.seshat = {
  enable = true;              # installs the flake's package
  theme = {                   # optional: a theme Seshat always starts with
    name = "Nord";
    extends = "dark";
    colors.surface = "#3b4252";
    colors.accent = "#88c0d0";
    typography.font_family = "Inter";
  };
};
```

| Option | Meaning |
|---|---|
| `enable` | Install Seshat |
| `package` | The package; defaults to this flake's. `null` installs nothing (for a Seshat installed some other way), but then no theme can be applied |
| `theme` | A theme with the keys of a [theme file](development/themes.md). Every value is optional and checked when the configuration is evaluated; unset values come from the `extends` base theme |
| `themeFile`, `finalPackage` | Read-only: the generated theme file, and the package that is installed |

With a `theme`, the installed `seshat` is a wrapper that always runs `seshat --theme-file <generated file>`. The theme is listed first in the theme menu under **Launch Theme** and wins at every launch, but it is never saved, so it doesn't change `settings.toml`. A running Seshat shows a new theme the next time it starts. The module doesn't manage `settings.toml`, because Seshat rewrites it whenever a setting changes (decisions S1–S3).

Install Seshat only through the module when you use a theme: a second, unwrapped `seshat` from `environment.systemPackages` may come first on `PATH`.

### Stylix

With [Stylix](https://github.com/nix-community/stylix) imported and enabled, the module fills `programs.seshat.theme` from your Stylix colours, sans-serif font, application font size and polarity, so Seshat starts with your Stylix theme ([specification](specifications/5-STYLIX-INTEGRATION.md), decisions S6–S10). Like Stylix's own targets it is on when `stylix.autoEnable` is true. Import Stylix yourself; Seshat's module never imports it.

```nix
programs.seshat = {
  enable = true;
  # stylix.enable = false;          # opt out
  # stylix.fonts.enable = false;    # keep Seshat's font; also colors.enable, polarity.enable
  theme.colors.accent = "#ff5555";  # your own values win over Stylix's, one by one
};
```

The module is tested in `nix/tests` (`nix flake check ./nix/tests`).

## What the package contains

| Path | Purpose |
|---|---|
| `bin/seshat` | the application (GTK wrapper around `.seshat-wrapped`) |
| `bin/seshat-progress` | example D-Bus client |
| `share/applications/dev.kdmorris.Seshat.desktop` | desktop entry (validated with `desktop-file-validate`) |
| `share/icons/hicolor/scalable/apps/dev.kdmorris.Seshat.svg` | application icon |
| `share/metainfo/dev.kdmorris.Seshat.metainfo.xml` | AppStream metadata (`appstreamcli validate --pedantic` passes; one pedantic note: the app id contains capitals) |
| `share/licenses/seshat/LICENSE` | the licence (GPL-3.0-or-later) |
| `share/doc/seshat/examples/settings.toml` | example settings: every setting (including `[fuzzy_find]`) and every configurable key binding, with their defaults; copy to `~/.config/seshat/settings.toml` |
| `share/doc/seshat/examples/themes/catppuccin-mocha.toml` | example theme (Catppuccin Mocha); copy to `~/.config/seshat/themes/` and pick it from the theme menu |

The build runs the whole workspace test suite in the Nix sandbox, including the D-Bus tests on a private bus.

## Dependencies

**Build:** Rust ≥ 1.93, `pkg-config`, GTK ≥ 4.20 and GLib ≥ 2.86 development files.

**Runtime:**
- GTK 4 and GLib/GIO
- `shared-mime-info`, for file types and icons; the Nix package bundles it
- GVfs (optional), for the Trash location, network mounts and the device list
- a desktop portal (optional), for opening files from a sandbox
- a Wayland compositor

## Wayland requirements

- **Seshat needs a Wayland compositor and does not need XWayland.**
- Nothing in the package sets `GDK_BACKEND`. The GTK wrapper only sets data directories, GIO modules and the pixbuf loader cache, so GTK picks the Wayland backend whenever `WAYLAND_DISPLAY` is set.
- Every launch logs the backend GTK actually connected to ("display backend verified").
- `seshat --check-backend` exits non-zero unless the backend is native Wayland.
- `SESHAT_BIN=./result/bin/seshat scripts/verify-wayland.sh` verifies the packaged binary in a headless compositor.

## D-Bus

- Seshat owns `dev.kdmorris.Seshat.Operations` on the **session bus** while it runs ([protocol](ipc/protocol.md)). GApplication separately owns the application id `dev.kdmorris.Seshat`.
- There is no D-Bus activation file. The service has nothing to report unless Seshat is running, and activating it would open a window. A future standalone operations daemon would add a `.service` file.
- No system-bus access and no polkit actions are needed.

## Permissions

- **Files:** the user's normal filesystem access. There is no setuid helper.
- **Session bus:** owns one name, and talks to GVfs and portals through GIO.
- **No X11, no network access** (apart from GVfs network mounts the user opens) and no system bus.

## Other formats (not yet built)

Notes for when these are added:

- **Flatpak:**
  - `--socket=wayland` and **no** `--socket=fallback-x11`; otherwise the app could silently run under XWayland.
  - `--filesystem=host` (a file manager needs it).
  - `--own-name=dev.kdmorris.Seshat.Operations` and `--talk-name=org.gtk.vfs.*`.
  - The Trash and device features depend on host GVfs access.
- **AppImage:** GTK4 AppImages bundle their own GTK and often break theming and portals. That's lower priority.
- **DEB/RPM:** these are straightforward from `cargo build --release` plus the `data/` files, using the dependencies listed above.
