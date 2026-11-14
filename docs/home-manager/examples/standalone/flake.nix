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
