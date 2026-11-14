# Checks of `programs.seshat.stylix` with the real Stylix modules and
# Base16 schemes from nixpkgs.
args@{ stylix, ... }:
let
  t = import ./lib.nix args;
  inherit (t)
    pkgs
    lib
    homeConfig
    expect
    that
    ;

  scheme = name: "${pkgs.base16-schemes}/share/themes/${name}.yaml";

  # Standalone Home Manager with Stylix and Seshat.
  withStylix =
    stylixSettings: modules:
    homeConfig (
      [
        stylix.homeModules.stylix
        {
          stylix = {
            enable = true;
          }
          // stylixSettings;
          programs.seshat.enable = true;
        }
      ]
      ++ modules
    );

  # The mapping the module should produce (decision S9), written out again
  # here so that changing it means changing a test.
  expectedColors =
    config:
    let
      c = config.lib.stylix.colors.withHashtag;
    in
    {
      surface = c.base00;
      app_background = c.base00;
      navigation_surface = c.base00;
      text_primary = c.base05;
      text_secondary = c.base04;
      text_muted = c.base03;
      accent = c.base0D;
      focus = c.base0D;
      border = c.base02;
      hover = c.base05 + "14";
      selected = c.base0D + "4d";
      error = c.base08;
      warning = c.base09;
      success = c.base0B;
    };

  # `has` lines for every colour, for checkTheme.
  hasColors =
    config:
    lib.concatStrings (
      lib.mapAttrsToList (token: value: "has '${token} = \"${value}\"'\n") (expectedColors config)
    );

  # A scheme checked end to end: mapping, polarity, Stylix's default fonts,
  # and Seshat accepting the file with no problems (which includes enough
  # contrast for the primary text).
  schemeCheck =
    name: stylixSettings: extendsLine:
    let
      config = withStylix stylixSettings [ ];
    in
    t.checkTheme name config ''
      ${hasColors config}
      has 'name = "Stylix"'
      lacks 'navigation_selected'
      ${extendsLine}
      has 'font_family = "${config.stylix.fonts.sansSerif.name}"'
      has 'font_size = 12'
    '';

  themeOf = config: config.programs.seshat.theme;
in
{
  stylix-dark = schemeCheck "stylix-dark" {
    base16Scheme = scheme "gruvbox-dark-hard";
    polarity = "dark";
  } "has 'extends = \"dark\"'";

  stylix-light = schemeCheck "stylix-light" {
    base16Scheme = scheme "gruvbox-light-hard";
    polarity = "light";
  } "has 'extends = \"light\"'";

  # A vivid accent; polarity "either" says nothing, so nothing is guessed.
  stylix-accent = schemeCheck "stylix-accent" {
    base16Scheme = scheme "outrun-dark";
  } "lacks 'extends'";

  stylix-fonts =
    let
      config = withStylix {
        base16Scheme = scheme "gruvbox-dark-hard";
        fonts.sansSerif = {
          name = "Inter";
          package = pkgs.inter;
        };
        fonts.sizes.applications = 13.6;
      } [ ];
    in
    t.checkTheme "stylix-fonts" config ''
      has 'font_family = "Inter"'
      has 'font_size = 14'
    '';

  # Precedence (decision S8): the user's own settings win over Stylix's,
  # value by value, and the rest still comes from Stylix.
  stylix-overrides =
    let
      config = withStylix { base16Scheme = scheme "gruvbox-dark-hard"; } [
        {
          programs.seshat.theme = {
            name = "Mine";
            colors.accent = "#ff5555";
            colors.border = null;
            typography.font_size = 10;
          };
        }
      ];
    in
    t.checkTheme "stylix-overrides" config ''
      has 'name = "Mine"'
      has 'accent = "#ff5555"'
      lacks 'border ='
      has 'font_size = 10'
      has 'surface = "${config.lib.stylix.colors.withHashtag.base00}"'
    '';

  # Each part can be turned off on its own.
  stylix-parts =
    let
      without =
        part:
        themeOf (
          withStylix {
            base16Scheme = scheme "gruvbox-dark-hard";
            polarity = "dark";
          } [ { programs.seshat.stylix.${part}.enable = false; } ]
        );
      noColors = without "colors";
      noFonts = without "fonts";
      noPolarity = without "polarity";
      allNull = lib.all (v: v == null) ;
    in
    expect "stylix-parts" [
      (that "colors.enable = false sets no colour" (allNull (lib.attrValues noColors.colors)))
      (that "  but keeps the fonts" (noColors.typography.font_size == 12))
      (that "fonts.enable = false sets no font" (allNull (lib.attrValues noFonts.typography)))
      (that "  but keeps the colours" (noFonts.colors.surface != null))
      (that "polarity.enable = false sets no base theme" (noPolarity.extends == null))
      (that "  but keeps the colours" (noPolarity.colors.surface != null))
    ];

  # Following Stylix's convention for its own targets.
  stylix-enabling =
    let
      on = withStylix { base16Scheme = scheme "gruvbox-dark-hard"; } [ ];
      autoOff = withStylix {
        base16Scheme = scheme "gruvbox-dark-hard";
        autoEnable = false;
      } [ ];
      optedOut = withStylix { base16Scheme = scheme "gruvbox-dark-hard"; } [
        { programs.seshat.stylix.enable = false; }
      ];
      stylixOff = homeConfig [
        stylix.homeModules.stylix
        { programs.seshat.enable = true; }
      ];
      plain = config: config.programs.seshat.theme == null && config.programs.seshat.finalPackage == t.package;
    in
    expect "stylix-enabling" [
      (that "on with Stylix and autoEnable" (on.programs.seshat.stylix.enable && themeOf on != null))
      (that "off with stylix.autoEnable = false" (plain autoOff))
      (that "off when the user opts out" (plain optedOut))
      (that "off while Stylix is imported but disabled" (plain stylixOff))
    ];

  # A size Seshat can't use is not passed on, and Home Manager says why.
  stylix-font-size-out-of-range =
    let
      config = withStylix {
        base16Scheme = scheme "gruvbox-dark-hard";
        fonts.sizes.applications = 40;
      } [ ];
    in
    expect "stylix-font-size-out-of-range" [
      (that "the size is left to Seshat" ((themeOf config).typography.font_size == null))
      (that "the family is still set" ((themeOf config).typography.font_family != null))
      (that "a warning explains it" (
        lib.any (lib.hasInfix "programs.seshat.theme.typography.font_size") config.warnings
      ))
    ];

  # NixOS with Stylix's NixOS module, which imports Stylix's Home Manager
  # module for every user: Seshat is themed without any Stylix setting in
  # the user's own configuration.
  stylix-nixos =
    let
      nixos = lib.nixosSystem {
        modules = [
          args.home-manager.nixosModules.home-manager
          stylix.nixosModules.stylix
          {
            nixpkgs.hostPlatform = args.system;
            boot.loader.grub.enable = false;
            fileSystems."/".device = "none";
            system.stateVersion = "25.05";
            users.users.alice.isNormalUser = true;
            stylix = {
              enable = true;
              base16Scheme = scheme "gruvbox-dark-hard";
              polarity = "dark";
            };
            home-manager.sharedModules = [ args.seshat.homeModules.default ];
            home-manager.users.alice = {
              home.stateVersion = "25.05";
              programs.seshat.enable = true;
            };
          }
        ];
      };
      user = nixos.config.home-manager.users.alice;
    in
    t.checkTheme "stylix-nixos" user ''
      ${hasColors user}
      has 'extends = "dark"'
    '';
}
