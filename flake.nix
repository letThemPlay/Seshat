{
  description = "Seshat — a native Wayland file explorer";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rustc
            cargo
            clippy
            rustfmt
            rust-analyzer
            pkg-config
            wrapGAppsHook4
          ];

          buildInputs = with pkgs; [
            glib
            gtk4
            libadwaita
            dbus
            wayland
            wayland-protocols
          ];

          packages = with pkgs; [
            weston
            wayland-utils
          ];

          # Fail loudly instead of silently falling back to X11/XWayland.
          GDK_BACKEND = "wayland";
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };
      });
}
