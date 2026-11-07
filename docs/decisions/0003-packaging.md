# ADR 0003: Nix package first

- **Status:** Accepted
- **Date:** 2026-11-07

## Context

Spec §16 asks to investigate Flatpak, AppImage, DEB and RPM, and to make one format work first. Development happens on NixOS, where Flatpak tooling isn't installed and a Flatpak build would mean downloading the GNOME runtime.

## Decision

The first working format is a **Nix flake package** (`packages.default`), chosen by the maintainer. It:

- builds both binaries and runs the full test suite in the sandbox, including D-Bus tests on a private bus
- installs the desktop entry, icon and AppStream metadata
- bundles `shared-mime-info`, so file types don't depend on the host
- adds no environment that could force X11. Verified: `--check-backend` and the headless Wayland verification pass on the packaged binary.

## Consequences

- Nix and NixOS users can install Seshat today, on any distribution with Nix.
- The Flatpak requirements (Wayland socket only, no X11 fallback, host filesystem access, GVfs and the D-Bus name) are documented in [packaging.md](../packaging.md) for when a second format is added.
