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
    {
      checks = nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" ] (
        system:
        import ./checks.nix {
          inherit
            system
            nixpkgs
            home-manager
            seshat
            ;
        }
        // import ./stylix-checks.nix {
          inherit
            system
            nixpkgs
            home-manager
            stylix
            seshat
            ;
        }
      );
    };
}
