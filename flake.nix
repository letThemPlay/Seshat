{
  description = "Seshat — a native Wayland file explorer";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        pkgs = import nixpkgs { inherit system; };
        manifest = (pkgs.lib.importTOML ./Cargo.toml).workspace.package;

        seshat = pkgs.rustPlatform.buildRustPackage {
          pname = "seshat";
          inherit (manifest) version;
          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./crates
              ./data
              ./docs/themes # an integration test reads the example theme
            ];
          };
          cargoLock.lockFile = ./Cargo.lock;

          # wrapGAppsHook4 sets XDG_DATA_DIRS/GSettings for icons and
          # schemas. It does not set GDK_BACKEND, so the app uses native
          # Wayland whenever WAYLAND_DISPLAY is available.
          nativeBuildInputs = with pkgs; [ pkg-config wrapGAppsHook4 ];
          # shared-mime-info: content types (icons, "Type" column) must not
          # depend on the host having a MIME database.
          buildInputs = with pkgs; [ glib gtk4 shared-mime-info ];

          cargoBuildFlags = [ "-p" "seshat" "-p" "seshat-progress" ];
          cargoTestFlags = [ "--workspace" ];
          # Lets the D-Bus integration tests run on a private bus instead of
          # being skipped.
          nativeCheckInputs = [ pkgs.dbus ];
          preCheck = ''
            export XDG_DATA_DIRS=${pkgs.shared-mime-info}/share''${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}
          '';

          postInstall = ''
            install -Dm644 data/dev.kdmorris.Seshat.desktop \
              $out/share/applications/dev.kdmorris.Seshat.desktop
            install -Dm644 data/icons/dev.kdmorris.Seshat.svg \
              $out/share/icons/hicolor/scalable/apps/dev.kdmorris.Seshat.svg
            install -Dm644 data/dev.kdmorris.Seshat.metainfo.xml \
              $out/share/metainfo/dev.kdmorris.Seshat.metainfo.xml
          '';

          meta = {
            description = "Native Wayland file manager with observable file operations";
            license = pkgs.lib.licenses.gpl3Plus;
            mainProgram = "seshat";
            platforms = pkgs.lib.platforms.linux;
          };
        };
      in
      {
        packages.default = seshat;
        packages.seshat = seshat;

        apps.default = {
          type = "app";
          program = pkgs.lib.getExe seshat;
        };
        apps.seshat-progress = {
          type = "app";
          program = "${seshat}/bin/seshat-progress";
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ seshat ];
          nativeBuildInputs = with pkgs; [
            clippy
            rustfmt
            rust-analyzer
          ];

          buildInputs = with pkgs; [
            dbus
            wayland
            wayland-protocols
          ];

          packages = with pkgs; [
            weston
            sway
            wtype
            grim
            bubblewrap
            wayland-utils
            desktop-file-utils
            appstream
          ];

          # Fail loudly instead of silently falling back to X11/XWayland.
          GDK_BACKEND = "wayland";
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };
      });
}
