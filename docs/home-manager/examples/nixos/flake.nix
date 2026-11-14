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
      nixosConfigurations.desktop = nixpkgs.lib.nixosSystem {
        modules = [
          ./configuration.nix
          home-manager.nixosModules.home-manager
          stylix.nixosModules.stylix
          {
            home-manager.sharedModules = [ seshat.homeModules.default ];
            home-manager.users.alice = ./home.nix;
          }
        ];
      };
    };
}
