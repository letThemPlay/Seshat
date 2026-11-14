# Checks of `programs.seshat` without Stylix. Each attribute is one check:
# evaluation-time expectations fail the evaluation with a message, the
# rest run in a derivation.
args:
let
  t = import ./lib.nix args;
  inherit (t)
    lib
    homeConfig
    installsSeshat
    evaluates
    packagesEvaluate
    expect
    that
    ;

  nord = {
    name = "Nord";
    extends = "dark";
    colors = {
      surface = "#3b4252";
      navigation_selected = "#434c5e";
      accent = "#88c0d0";
      hover = "#ffffff12";
    };
    geometry.border_radius = 6;
    typography = {
      font_family = "Inter";
      font_size = 11;
    };
  };

  themed = homeConfig [
    {
      programs.seshat = {
        enable = true;
        theme = nord;
      };
    }
  ];
in
{
  disabled =
    let
      config = homeConfig [ { programs.seshat.theme = nord; } ];
    in
    expect "disabled" [
      (that "nothing is installed when disabled" (!installsSeshat config))
      (that "the configuration evaluates" (packagesEvaluate config))
    ];

  without-theme =
    let
      config = homeConfig [ { programs.seshat.enable = true; } ];
    in
    expect "without-theme" [
      (that "the flake's package is installed" (installsSeshat config))
      (that "it is not wrapped" (config.programs.seshat.finalPackage == t.package))
      (that "there is no theme file" (config.programs.seshat.themeFile == null))
      (that "Stylix is off without Stylix" (!config.programs.seshat.stylix.enable))
    ];

  # Only the values that were set are written; the rest of the package is
  # untouched.
  theme = t.checkTheme "theme" themed ''
    has 'accent = "#88c0d0"'
    has 'border_radius = 6'
    has 'navigation_selected = "#434c5e"'
    lacks 'border ='
    test -e ${themed.programs.seshat.finalPackage}/bin/seshat-progress
    test -e ${themed.programs.seshat.finalPackage}/share/applications/dev.kdmorris.Seshat.desktop
  '';

  # Normal Nix module precedence: a plain definition beats `mkDefault`
  # (which the Stylix part uses), and `null` leaves a value to the base.
  precedence =
    let
      config = homeConfig [
        {
          programs.seshat = {
            enable = true;
            theme.colors = {
              accent = lib.mkDefault "#111111";
              border = lib.mkDefault "#111111";
              surface = lib.mkDefault "#111111";
            };
          };
        }
        {
          programs.seshat.theme.colors = {
            accent = "#222222";
            border = null;
          };
        }
      ];
      colors = config.programs.seshat.theme.colors;
    in
    expect "precedence" [
      (that "a plain definition wins over mkDefault" (colors.accent == "#222222"))
      (that "null overrides a default" (colors.border == null))
      (that "an unset value keeps the default" (colors.surface == "#111111"))
    ];

  # Mistakes are caught when the configuration is evaluated, not left for
  # Seshat to warn about.
  invalid-values =
    let
      themeWith =
        theme:
        (homeConfig [
          {
            programs.seshat = {
              enable = true;
              inherit theme;
            };
          }
        ]).programs.seshat.theme;
    in
    expect "invalid-values" [
      (that "a colour must be #rrggbb or #rrggbbaa" (!evaluates (themeWith { colors.accent = "red"; })))
      (that "unknown colours are rejected" (!evaluates (themeWith { colors.sparkle = "#ffffff"; })))
      (that "unknown settings are rejected" (!evaluates (themeWith { mystery = 1; })))
      (that "numbers must be in range" (!evaluates (themeWith { geometry.border_radius = 100; })))
      (that "font weights are hundreds" (!evaluates (themeWith { typography.font_weight = 450; })))
      (that "extends is light or dark" (!evaluates (themeWith { extends = "neon"; })))
      (that "a valid theme evaluates" (evaluates (themeWith nord)))
    ];

  # Home Manager throws "Failed assertions: - programs.seshat.theme needs
  # programs.seshat.package …" as soon as the configuration is read, so
  # only the failure itself can be checked here.
  theme-without-package =
    let
      config = homeConfig [
        {
          programs.seshat = {
            enable = true;
            package = null;
            theme = nord;
          };
        }
      ];
    in
    expect "theme-without-package" [
      (that "the configuration is refused" (!packagesEvaluate config))
    ];

  # Enabling the Stylix part without Stylix fails with an explanation
  # ("programs.seshat.stylix.enable needs Stylix: …").
  stylix-without-stylix =
    let
      config = homeConfig [
        {
          programs.seshat = {
            enable = true;
            stylix.enable = true;
          };
        }
      ];
    in
    expect "stylix-without-stylix" [
      (that "the configuration is refused" (!packagesEvaluate config))
    ];

  # The same module through Home Manager inside NixOS.
  nixos =
    let
      nixos = lib.nixosSystem {
        modules = [
          args.home-manager.nixosModules.home-manager
          {
            nixpkgs.hostPlatform = args.system;
            boot.loader.grub.enable = false;
            fileSystems."/".device = "none";
            system.stateVersion = "25.05";
            users.users.alice.isNormalUser = true;
            home-manager.sharedModules = [ args.seshat.homeModules.default ];
            home-manager.users.alice = {
              home.stateVersion = "25.05";
              programs.seshat = {
                enable = true;
                theme = nord;
              };
            };
          }
        ];
      };
    in
    t.checkTheme "nixos" nixos.config.home-manager.users.alice ''
      has 'accent = "#88c0d0"'
    '';
}
