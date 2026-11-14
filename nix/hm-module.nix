# Home Manager module for Seshat: `programs.seshat`.
#
# Installs Seshat and, when `programs.seshat.theme` is set, wraps it so it
# always starts with that theme (`seshat --theme-file <file>`, decision S1).
# The theme is an ordinary Seshat theme file (docs/development/themes.md),
# validated by Seshat itself; this module only writes it.
#
# `settings.toml` is deliberately not managed: Seshat rewrites it whenever
# a setting changes (decision S3).
self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  inherit (lib) mkOption types;

  cfg = config.programs.seshat;
  tomlFormat = pkgs.formats.toml { };

  # Every key of a theme file, with what it accepts. Mirrors
  # crates/seshat-presentation/src/theme/custom.rs; the checks in nix/tests
  # run `seshat --check-theme` on generated files, so the two cannot drift
  # apart unnoticed (decision S4).
  colorTokens = {
    app_background = "Window, toolbar and status bar.";
    surface = "Main content pane and the selected navigation item (one token by design).";
    navigation_surface = "Navigation pane.";
    text_primary = "Primary text.";
    text_secondary = "Secondary text.";
    text_muted = "Muted text.";
    accent = "Selected-item icon, checked menu items.";
    border = "Outlines and separators.";
    hover = "Hovered rows and buttons.";
    selected = "Selected files.";
    focus = "Keyboard focus outline.";
    error = "Error messages.";
    warning = "Warnings.";
    success = "Success messages.";
  };

  geometryRanges = {
    border_radius = [ 0 24 ];
    padding = [ 0 32 ];
    spacing = [ 0 32 ];
    row_height = [ 20 96 ];
    toolbar_height = [ 32 96 ];
    icon_size = [ 12 64 ];
  };

  hexColor = types.strMatching "#([0-9a-fA-F]{6}|[0-9a-fA-F]{8})";

  # An option that is left out of the file when null, so the value comes
  # from the base theme (`extends`).
  optional =
    type: description:
    mkOption {
      type = types.nullOr type;
      default = null;
      inherit description;
    };

  section =
    options:
    mkOption {
      type = types.submodule { inherit options; };
      default = { };
    };

  themeType = types.submodule {
    options = {
      name = optional types.nonEmptyStr "Name shown in the theme menu.";
      extends = optional (types.enum [
        "light"
        "dark"
      ]) "Base theme for every value not set here. Seshat's default is `light`.";
      colors = section (
        lib.mapAttrs (
          _: description: optional hexColor "${description} `#rrggbb` or `#rrggbbaa`."
        ) colorTokens
      );
      geometry = section (
        lib.mapAttrs (
          name: range:
          optional (types.ints.between (lib.head range) (lib.last range)) "`${name}` in pixels."
        ) geometryRanges
      );
      typography = section {
        font_family = optional types.nonEmptyStr "Font family.";
        font_size = optional (types.ints.between 6 36) "Font size in points.";
        font_weight = optional (types.enum (
          lib.genList (i: (i + 1) * 100) 9
        )) "Font weight: 100, 200, … 900.";
        line_height = optional (types.numbers.between 0.8 3.0) "Line height, as a multiple of the font size.";
      };
    };
  };

  # The theme as written to the file: unset values and empty sections are
  # left out.
  themeSettings =
    theme:
    lib.filterAttrs (_: value: value != null && value != { }) (
      lib.mapAttrs (
        _: value: if lib.isAttrs value then lib.filterAttrs (_: v: v != null) value else value
      ) theme
    );

  themeFile =
    if cfg.theme == null then null else tomlFormat.generate "seshat-theme.toml" (themeSettings cfg.theme);

  wrapped = pkgs.symlinkJoin {
    name = "seshat-themed-${cfg.package.version or "unknown"}";
    paths = [ cfg.package ];
    nativeBuildInputs = [ pkgs.makeBinaryWrapper ];
    # The flag comes before any argument the user gives, which Seshat
    # accepts (`--theme-file FILE --check-backend` and so on).
    postBuild = ''
      wrapProgram $out/bin/seshat --add-flags ${lib.escapeShellArg "--theme-file ${themeFile}"}
    '';
    meta = cfg.package.meta or { };
  };
in
{
  _file = ./hm-module.nix;

  options.programs.seshat = {
    enable = lib.mkEnableOption "Seshat, a native Wayland file manager";

    package = mkOption {
      type = types.nullOr types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default or null;
      defaultText = lib.literalExpression "seshat.packages.\${pkgs.stdenv.hostPlatform.system}.default";
      description = ''
        The Seshat package. `null` installs nothing, for a Seshat installed
        some other way; a theme then cannot be applied, because it is passed
        by wrapping the package.
      '';
    };

    theme = mkOption {
      type = types.nullOr themeType;
      default = null;
      example = lib.literalExpression ''
        {
          name = "Nord";
          extends = "dark";
          colors.surface = "#3b4252";
          colors.accent = "#88c0d0";
          typography.font_family = "Inter";
        }
      '';
      description = ''
        A theme Seshat always starts with (`seshat --theme-file`). It is
        listed first in the theme menu under "Launch Theme" and wins over
        the theme saved in the app, but it is never saved there. Values left
        unset come from the `extends` base theme. See
        docs/development/themes.md for every key.
      '';
    };

    themeFile = mkOption {
      type = types.nullOr types.path;
      readOnly = true;
      default = themeFile;
      defaultText = lib.literalMD "the generated theme file, or `null` without a theme";
      description = "The generated theme file passed to Seshat.";
    };

    finalPackage = mkOption {
      type = types.nullOr types.package;
      readOnly = true;
      default = if cfg.theme == null || cfg.package == null then cfg.package else wrapped;
      defaultText = lib.literalMD "`package`, wrapped to pass `themeFile` when a theme is set";
      description = "The package that is installed.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.theme == null || cfg.package != null;
        message = ''
          programs.seshat.theme needs programs.seshat.package: the theme is
          passed by wrapping the package, so it cannot be null. Install Seshat
          through Home Manager instead of some other way.
        '';
      }
    ];

    home.packages = lib.optional (cfg.finalPackage != null) cfg.finalPackage;
  };
}
