# Stylix support for `programs.seshat` (specification 5).
#
# Fills `programs.seshat.theme` from Stylix's palette, fonts and polarity.
# Every value is set with `mkDefault`, so a plain definition of the same
# option wins (decision S8: user settings, then Stylix, then the `extends`
# base theme). Stylix is only read while Home Manager evaluates the
# configuration: the result is an ordinary Seshat theme file.
#
# This module never imports or installs Stylix (or its fonts); the user
# imports Stylix as usual.
{
  config,
  options,
  lib,
  ...
}:
let
  cfg = config.programs.seshat.stylix;

  # Stylix's Home Manager module is imported (standalone, or by Stylix's
  # NixOS module through home-manager.sharedModules).
  hasStylix = options ? stylix && options.stylix ? enable;
  stylixOn = hasStylix && config.stylix.enable;

  colors = config.lib.stylix.colors.withHashtag;
  # `base0D` with an alpha suffix: `#rrggbb` + `aa`.
  withAlpha = base: alpha: colors.${base} + alpha;

  # Decision S9: Base16 roles onto Seshat's colour tokens. base01, base03, base06,
  # base07, base0A, base0C, base0E and base0F have no matching token, and
  # navigation_selected is left unset so the selected item follows surface.
  # Every background the main text sits on is base00, the scheme's own text
  # background: base01 under text fails the contrast check for 25 of the 303
  # schemes in nixpkgs. Borders (base02) separate the areas, and the selected
  # navigation item keeps its outline and accent icon.
  palette = {
    surface = colors.base00; # content pane and selected navigation item
    app_background = colors.base00;
    navigation_surface = colors.base00;
    text_primary = colors.base05;
    # Base16 has two text colours, base05 and base04 (base03 is for comments
    # and reaches 3:1 against base00 in only 115 of 303 schemes; base04 in
    # 267). Secondary text is base05 so it is never fainter than muted text.
    text_secondary = colors.base05;
    text_muted = colors.base04;
    accent = colors.base0D;
    focus = colors.base0D;
    border = colors.base02;
    hover = withAlpha "base05" "14"; # faint foreground, either polarity
    selected = withAlpha "base0D" "4d"; # translucent accent
    error = colors.base08;
    warning = colors.base09; # yellow (base0A) is unreadable on light schemes
    success = colors.base0B;
  };

  fontSize =
    let
      size = config.stylix.fonts.sizes.applications;
    in
    if builtins.isFloat size then builtins.floor (size + 0.5) else size;
  fontSizeFits = fontSize >= 6 && fontSize <= 36;

  polarity = config.stylix.polarity;
in
{
  _file = ./stylix.nix;

  options.programs.seshat.stylix = {
    enable = lib.mkOption {
      type = lib.types.bool;
      default = stylixOn && config.stylix.autoEnable;
      defaultText = lib.literalExpression "config.stylix.enable && config.stylix.autoEnable, if Stylix is imported";
      example = false;
      description = ''
        Theme Seshat from Stylix: fills `programs.seshat.theme`, which Seshat
        then always starts with. Like Stylix's own targets, this is on when
        Stylix is enabled and `stylix.autoEnable` is true, and off when
        Stylix isn't imported.
      '';
    };
    colors.enable = lib.mkEnableOption "Stylix's colours" // {
      default = true;
    };
    fonts.enable = lib.mkEnableOption "Stylix's sans-serif font and application font size" // {
      default = true;
    };
    polarity.enable = lib.mkEnableOption "Stylix's polarity as the base theme (`extends`)" // {
      default = true;
    };
  };

  config = lib.mkIf cfg.enable (
    lib.mkMerge [
      {
        assertions = [
          {
            assertion = stylixOn;
            message = ''
              programs.seshat.stylix.enable needs Stylix: import Stylix's Home
              Manager module (or its NixOS module, which imports it for Home
              Manager) and set stylix.enable = true. Seshat's module does not
              import Stylix itself.
            '';
          }
        ];
      }
      (lib.mkIf stylixOn {
        programs.seshat.theme.name = lib.mkDefault "Stylix";
      })
      (lib.mkIf (stylixOn && cfg.colors.enable) {
        programs.seshat.theme.colors = lib.mapAttrs (_: lib.mkDefault) palette;
      })
      (lib.mkIf (stylixOn && cfg.fonts.enable) {
        programs.seshat.theme.typography = {
          font_family = lib.mkDefault config.stylix.fonts.sansSerif.name;
          font_size = lib.mkIf fontSizeFits (lib.mkDefault fontSize);
        };
        warnings = lib.optional (!fontSizeFits) ''
          Stylix's application font size (stylix.fonts.sizes.applications =
          ${toString config.stylix.fonts.sizes.applications}) is outside the 6 to 36 points Seshat
          accepts, so Seshat keeps its default size. Set
          programs.seshat.theme.typography.font_size to choose one.
        '';
      })
      # "either" says nothing about light or dark, so nothing is set and
      # polarity is never guessed from the colours.
      (lib.mkIf (stylixOn && cfg.polarity.enable && polarity != "either") {
        programs.seshat.theme.extends = lib.mkDefault polarity;
      })
    ]
  );
}
