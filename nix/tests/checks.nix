# Each attribute is one check. Evaluation-time expectations fail the
# evaluation with a message; the rest run in a derivation.
{
  system,
  nixpkgs,
  home-manager,
  seshat,
}:
let
  pkgs = nixpkgs.legacyPackages.${system};
  inherit (nixpkgs) lib;
  package = seshat.packages.${system}.default;

  base = {
    home.username = "alice";
    home.homeDirectory = "/home/alice";
    home.stateVersion = "25.05";
  };

  # The configuration of a Home Manager setup with these modules.
  homeConfig =
    modules:
    (home-manager.lib.homeManagerConfiguration {
      inherit pkgs;
      modules = [
        seshat.homeModules.default
        base
      ]
      ++ modules;
    }).config;

  failedAssertions = config: map (a: a.message) (lib.filter (a: !a.assertion) config.assertions);
  installsSeshat = config: lib.any (p: lib.hasPrefix "seshat" (p.name or "")) config.home.packages;
  evaluates = value: (builtins.tryEval (builtins.deepSeq value true)).success;

  # A check made only of evaluation-time expectations.
  expect =
    name: expectations:
    let
      failed = lib.filter (e: !e.ok) expectations;
    in
    if failed == [ ] then
      pkgs.runCommand "seshat-hm-${name}" { } "touch $out"
    else
      throw "${name}: ${lib.concatMapStringsSep "; " (e: e.what) failed}";

  that = what: ok: { inherit what ok; };

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
  themedSeshat = themed.programs.seshat;
in
{
  disabled =
    let
      config = homeConfig [ { programs.seshat.theme = nord; } ];
    in
    expect "disabled" [
      (that "nothing is installed when disabled" (!installsSeshat config))
      (that "no assertion fails" (failedAssertions config == [ ]))
    ];

  without-theme =
    let
      config = homeConfig [ { programs.seshat.enable = true; } ];
    in
    expect "without-theme" [
      (that "the flake's package is installed" (installsSeshat config))
      (that "it is not wrapped" (config.programs.seshat.finalPackage == package))
      (that "there is no theme file" (config.programs.seshat.themeFile == null))
    ];

  # The wrapper passes the generated theme, and Seshat itself accepts the
  # file without a single warning (decision S4).
  theme =
    assert lib.assertMsg (installsSeshat themed) "the wrapped package is installed";
    assert lib.assertMsg (failedAssertions themed == [ ]) "no assertion fails";
    pkgs.runCommand "seshat-hm-theme" { } ''
      set -euo pipefail
      theme=${themedSeshat.themeFile}
      bin=${themedSeshat.finalPackage}/bin/seshat
      cat "$theme"

      # The wrapper always passes this file.
      grep -aqF -- "--theme-file" "$bin"
      grep -aqF -- "$theme" "$bin"

      # Seshat loads it exactly as it would at launch, with no problems.
      "$bin" --check-theme "$theme"

      # Only the values that were set, and nothing tied to the Nix store
      # or to Home Manager: Seshat reads it on its own.
      grep -qx 'accent = "#88c0d0"' "$theme"
      grep -qx 'border_radius = 6' "$theme"
      grep -qx 'navigation_selected = "#434c5e"' "$theme"
      # (`! grep` would not stop the script under `set -e`.)
      if grep -q 'border =' "$theme"; then echo "unset values are written"; exit 1; fi
      if grep -q '/nix/store' "$theme"; then echo "store path in the theme"; exit 1; fi

      # The rest of the package is untouched.
      test -e ${themedSeshat.finalPackage}/bin/seshat-progress
      test -e ${themedSeshat.finalPackage}/share/applications/dev.kdmorris.Seshat.desktop
      touch $out
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
      themeWith = theme: (homeConfig [ { programs.seshat = { enable = true; inherit theme; }; } ]).programs.seshat.theme;
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
    # Home Manager throws "Failed assertions: - programs.seshat.theme needs
    # programs.seshat.package …" as soon as the configuration is read, so
    # only the failure itself can be checked here.
    expect "theme-without-package" [
      (that "the configuration is refused" (!evaluates config.home.packages))
    ];

  # The same module through Home Manager inside NixOS.
  nixos =
    let
      nixos = nixpkgs.lib.nixosSystem {
        modules = [
          home-manager.nixosModules.home-manager
          {
            nixpkgs.hostPlatform = system;
            boot.loader.grub.enable = false;
            fileSystems."/".device = "none";
            system.stateVersion = "25.05";
            users.users.alice.isNormalUser = true;
            home-manager.sharedModules = [ seshat.homeModules.default ];
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
      user = nixos.config.home-manager.users.alice;
    in
    assert lib.assertMsg (installsSeshat user) "the wrapped package is installed for the user";
    pkgs.runCommand "seshat-hm-nixos" { } ''
      grep -aqF -- "${user.programs.seshat.themeFile}" ${user.programs.seshat.finalPackage}/bin/seshat
      touch $out
    '';
}
