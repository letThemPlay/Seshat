# The examples in docs/home-manager are tested as they are (spec 5 §13):
# their flakes' `outputs` are called with this flake's inputs, and the
# guide must show every example file verbatim.
args@{
  system,
  nixpkgs,
  home-manager,
  stylix,
  seshat,
  ...
}:
let
  t = import ./lib.nix args;
  inherit (t) lib expect that;

  examples = seshat.outPath + "/docs/home-manager/examples";
  guide = builtins.readFile (seshat.outPath + "/docs/home-manager/README.md");

  outputsOf =
    dir:
    (import "${examples}/${dir}/flake.nix").outputs {
      inherit
        nixpkgs
        home-manager
        stylix
        seshat
        ;
    };

  standalone = (outputsOf "standalone").homeConfigurations.alice.config;

  # The example leaves the hardware to the user's own configuration; the
  # test supplies the minimum NixOS needs to evaluate.
  nixos =
    ((outputsOf "nixos").nixosConfigurations.desktop.extendModules {
      modules = [
        {
          nixpkgs.hostPlatform = system;
          boot.loader.grub.enable = false;
          fileSystems."/".device = "none";
        }
      ];
    }).config.home-manager.users.alice;

  exampleFiles = [
    "standalone/flake.nix"
    "standalone/home.nix"
    "nixos/flake.nix"
    "nixos/configuration.nix"
    "nixos/home.nix"
  ];
in
# The examples are written for x86_64-linux.
lib.optionalAttrs (system == "x86_64-linux") {
  example-standalone = t.checkTheme "example-standalone" standalone ''
    has 'name = "Stylix"'
    has 'extends = "dark"'
    has 'surface = "${standalone.lib.stylix.colors.withHashtag.base00}"'
    has 'text_muted = "${standalone.lib.stylix.colors.withHashtag.base04}"'
  '';

  example-nixos = t.checkTheme "example-nixos" nixos ''
    has 'extends = "light"'
    has 'surface = "${nixos.lib.stylix.colors.withHashtag.base00}"'
  '';

  example-guide = expect "example-guide" (
    map (
      file:
      that "the guide shows ${file} as it is" (
        lib.hasInfix (builtins.readFile "${examples}/${file}") guide
      )
    ) exampleFiles
  );
}
