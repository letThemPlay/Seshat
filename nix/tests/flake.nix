# Checks for Seshat's Home Manager module (decision S7).
#
# A separate flake, so Home Manager and Stylix are inputs only here and
# the main flake keeps just nixpkgs and flake-utils.
#
#   nix flake check ./nix/tests
{
  description = "Checks for Seshat's Home Manager module";

  inputs = {
    seshat.url = "path:../..";
    nixpkgs.follows = "seshat/nixpkgs";
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    stylix = {
      url = "github:nix-community/stylix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      seshat,
      nixpkgs,
      home-manager,
      stylix,
      ...
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      argsFor = system: {
        inherit
          system
          nixpkgs
          home-manager
          stylix
          seshat
          ;
      };
    in
    {
      checks = nixpkgs.lib.genAttrs systems (
        system:
        import ./checks.nix (argsFor system)
        // import ./stylix-checks.nix (argsFor system)
        // import ./examples.nix (argsFor system)
      );

      # The visual scenarios' theme files (scripts/ui/stylix-themes.sh).
      packages = nixpkgs.lib.genAttrs systems (system: {
        stylix-themes = import ./visual.nix (argsFor system);
      });
    };
}
