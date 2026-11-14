# Helpers shared by the checks.
{
  system,
  nixpkgs,
  home-manager,
  seshat,
  ...
}:
let
  pkgs = nixpkgs.legacyPackages.${system};
  inherit (nixpkgs) lib;
in
rec {
  inherit pkgs lib;
  package = seshat.packages.${system}.default;

  base = {
    home.username = "alice";
    home.homeDirectory = "/home/alice";
    home.stateVersion = "25.05";
  };

  # The configuration of a standalone Home Manager setup with these modules.
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

  installsSeshat = config: lib.any (p: lib.hasPrefix "seshat" (p.name or "")) config.home.packages;
  evaluates = value: (builtins.tryEval (builtins.deepSeq value true)).success;
  # Whether the configuration's packages evaluate. Forces their store paths
  # only: `deepSeq` on packages walks their whole attribute trees.
  packagesEvaluate = config: evaluates (map (p: p.drvPath) config.home.packages);

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

  # A build-time check of a configuration's wrapper and theme file: the
  # wrapper passes the file, and Seshat accepts it without a single
  # problem (decision S4). `script` adds checks on "$theme".
  checkTheme =
    name: config: script:
    let
      cfg = config.programs.seshat;
    in
    assert lib.assertMsg (installsSeshat config) "${name}: the wrapped package is installed";
    pkgs.runCommand "seshat-hm-${name}" { } ''
      set -euo pipefail
      theme=${cfg.themeFile}
      bin=${cfg.finalPackage}/bin/seshat
      cat "$theme"
      grep -aqF -- "--theme-file" "$bin"
      grep -aqF -- "$theme" "$bin"
      "$bin" --check-theme "$theme"
      # Nothing tied to the Nix store: Seshat reads the file on its own.
      # (`! grep` would not stop the script under `set -e`.)
      if grep -q '/nix/store' "$theme"; then echo "store path in the theme"; exit 1; fi
      has() { grep -qxF -- "$1" "$theme" || { echo "missing: $1"; exit 1; }; }
      lacks() { if grep -qF -- "$1" "$theme"; then echo "unexpected: $1"; exit 1; fi; }
      ${script}
      touch $out
    '';
}
