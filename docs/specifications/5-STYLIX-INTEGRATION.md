# Feature Specification: Stylix Integration for NixOS and Home Manager

## 1. Overview

Introduce an optional Stylix integration for the file manager, allowing users to inherit their system-wide or Home Manager theme configuration automatically.

The integration should translate Stylix-provided colours, fonts and other supported theme properties into the file manager's existing theme configuration.

The goal is to make the file manager feel like a natural part of a user's Linux desktop without requiring them to configure the same theme values twice.

The integration must:
- Support NixOS configurations using Home Manager.
- Support standalone Home Manager configurations.
- Follow Stylix conventions and module architecture.
- Respect the application's existing theme system.
- Allow users to override individual theme values.
- Avoid introducing a runtime dependency on Stylix.
- Remain optional for users who do not use Nix or Stylix.

## 2. Goals

1. Automatically apply the user's Stylix colour scheme to the file manager.
2. Apply Stylix font configuration where supported by the application.
3. Translate Stylix theme values into the application's own theme format.
4. Provide sensible defaults while allowing granular user overrides.
5. Integrate cleanly with Nix flakes and Home Manager.
6. Preserve the application's existing themeability and custom theme support.
7. Ensure generated configuration is reproducible and declarative.

## 3. Non-goals

The initial implementation should not:
- Require Stylix to run the application.
- Make Stylix mandatory for Nix users.
- Replace the application's internal theme system.
- Modify the user's desktop environment theme.
- Automatically configure unrelated applications.
- Assume every Stylix option has a corresponding application setting.
- Introduce a separate theme engine solely for Stylix.
- Require users to use a particular desktop environment.

## 4. Module architecture and distribution

Inspect the existing repository and Nix flake structure before deciding the exact implementation.

The integration should expose a reusable Home Manager module through the project's flake outputs.

A proposed interface is:

```nix
{
  inputs.seshat.url = "github:OWNER/REPOSITORY";

  outputs = { self, ... }: {
    # Existing outputs...

    homeManagerModules.default = ./nix/modules/stylix.nix;
  };
}
```

The actual output naming must follow the repository's existing conventions. Do not replace or break existing flake outputs.

### Supported configuration environments

**Standalone Home Manager**

Users should be able to import the file manager's Home Manager module alongside Stylix's Home Manager module.

**NixOS with Home Manager**

Users should be able to use the same Home Manager module through their existing NixOS/Home Manager integration.

The implementation should not unnecessarily duplicate configuration logic between NixOS and Home Manager.

A separate NixOS module should only be introduced if the existing project architecture has a clear reason to provide one.

### Stylix integration

The module should consume Stylix configuration through the supported module interface.

It must not independently import or install Stylix.

Users remain responsible for importing and enabling Stylix in their Nix configuration.

The module should fail clearly or provide an appropriate evaluation-time diagnostic if it is enabled without the required Stylix configuration.

The precise dependency-checking approach should be validated against current Home Manager and Stylix module conventions.

## 5. Module options

Provide a dedicated configuration namespace for the file manager's Stylix integration.

Proposed interface:

```nix
programs.seshat.stylix = {
  enable = true;

  colors.enable = true;
  fonts.enable = true;

  polarity.enable = true;
};
```

This is an initial proposal, not a requirement to implement these exact option names. Inspect existing Home Manager conventions and the application's configuration before finalising the interface.

The integration should support:

- An overall enable/disable option.
- Independent colour integration.
- Independent font integration.
- Optional polarity/light-dark integration, if the application supports it.
- Granular overrides for individual generated theme values.

Disabling the integration must leave the user's existing application theme configuration untouched.

## 6. Colour mapping

Map the Stylix Base16 colour palette into the application's existing semantic theme roles.

The mapping must be based on the actual theme model in the Rust application, not on assumptions about its UI framework.

Suggested semantic mapping:

| Application role | Suggested Stylix value |
|---|---|
| Main workspace background | `base00` |
| Panel background | `base01` |
| Selected item background | `base02` |
| Muted text | `base03` |
| Secondary text | `base04` |
| Primary text | `base05` |
| Emphasised text | `base06` |
| Elevated surface | `base07` |
| Error | `base08` |
| Warning / orange accent | `base09` |
| Highlight / yellow accent | `base0A` |
| Success | `base0B` |
| Secondary accent | `base0C` |
| Primary accent | `base0D` |
| Alternative accent | `base0E` |
| Additional accent | `base0F` |

These mappings are starting points. Claude must compare them with the application's actual colour roles and propose any changes needed to preserve visual consistency.

### Important visual requirements

The application already has specific visual requirements that must remain intact.

In particular:
- The selected navigation item must retain its 12px rounded corners.
- The selected location's background must match the main pane background.
- Text and icons must maintain sufficient contrast.
- Selection, hover, focus and active states must remain distinguishable.
- Error, warning and success states must remain visually recognisable.
- The spatial panel layout must remain readable when using both light and dark schemes.

Do not simply apply Base16 values indiscriminately. The result should look like a deliberately designed application theme.

### Colour conversion

Use the colour values exposed by Stylix through:

```nix
config.lib.stylix.colors
```

Use the appropriate representation for the application's configuration format.

Avoid unnecessary custom colour conversion code if Stylix already provides the required format.

## 7. Font integration

Where supported by the application's theme configuration, map Stylix font settings to the corresponding application font roles.

Consider:
- Application/interface font.
- Monospace font, if used by the application.
- Font sizes.
- Font weights, if supported.
- Fallback font handling.

The implementation must inspect the existing font configuration and determine which Stylix font values are appropriate.

Do not assume that every font setting exposed by Stylix should be applied to every application text element.

Users must retain the ability to override font settings independently.

## 8. Light and dark polarity

If the application has explicit light and dark theme modes, investigate whether Stylix polarity can be used to configure them.

The integration should:
- Respect the configured Stylix polarity.
- Avoid incorrectly inferring polarity from individual colour values.
- Preserve the application's existing automatic or manual theme behaviour where appropriate.
- Allow users to override the resulting theme mode.

If the application already supports automatic theme selection through a separate mechanism, determine how the two systems should interact before implementing polarity support.

## 9. User overrides and precedence

Users must be able to customise individual values without disabling the entire Stylix integration.

Establish and document a predictable configuration precedence.

Recommended precedence:

1. Explicit user overrides.
2. Values generated by the Stylix integration.
3. Application defaults.

The module should use normal Nix module merging wherever possible.

Avoid generating an entire configuration file in a way that prevents users from overriding individual settings through Home Manager.

If the application requires a generated configuration file, investigate whether it can be assembled using existing Nix module options or structured configuration generation.

## 10. Runtime independence

Stylix must only be required when evaluating and building the user's Nix configuration.

The compiled file manager must not:
- Link against Stylix.
- Call Stylix commands at runtime.
- Read Stylix-specific files at runtime.
- Require a running Stylix service.
- Depend on environment variables that are only available under a particular desktop environment.

The generated application configuration should be sufficient for the application to use its theme independently.

This ensures that users can run the application outside their Nix-managed environment too.

## 11. Theme system compatibility

The Stylix integration must use the application's existing theme architecture.

It must not create a parallel theme system.

The integration should produce the same effective theme configuration that a user could configure manually through the application's supported theme configuration.

If the existing theme system does not expose sufficient semantic roles, identify the gap and propose the smallest appropriate extension.

Any necessary Rust changes must preserve Clean Architecture boundaries.

Nix-specific integration logic should remain outside the core domain and application layers.

## 12. Packaging and flake integration

Inspect the existing flake before introducing new outputs or restructuring anything.

The implementation should:
- Preserve existing packages and development shells.
- Preserve existing NixOS or Home Manager outputs.
- Expose the new Home Manager module using an appropriate flake output.
- Support the repository's existing system architectures.
- Avoid unnecessary dependencies.
- Follow the project's existing formatting and linting conventions.

The module should use the package already defined by the project's flake where appropriate rather than duplicating package definitions.

## 13. Documentation

Provide documentation covering:

1. What the Stylix integration does.
2. Supported configuration environments.
3. Installation and flake input configuration.
4. Importing the Home Manager module.
5. Enabling the integration.
6. Colour mapping.
7. Font mapping.
8. User overrides.
9. Disabling individual integration features.
10. Known limitations.

Include working examples for standalone Home Manager and NixOS with Home Manager.

The examples must be tested against the actual flake outputs and module options.

## 14. Testing and acceptance criteria

### Nix evaluation tests

- The Home Manager module evaluates successfully.
- The module works with Stylix enabled.
- Existing flake outputs remain valid.
- The module behaves correctly when disabled.
- Explicit user overrides take precedence.
- Individual colour and font integration options work independently.
- Invalid or missing required configuration produces a clear diagnostic.

### Build tests

- The application builds with generated Stylix configuration.
- The generated configuration uses valid application theme values.
- The application does not gain a runtime dependency on Stylix.

### Visual tests

Test at least:
- A dark Stylix scheme.
- A light Stylix scheme.
- A scheme with a strongly contrasting accent colour.
- A configuration with custom font settings.
- A configuration with explicit user overrides.

Verify that the spatial interface remains readable and that existing visual design requirements are preserved.

### Acceptance criteria

The feature is complete when:

- Users can import the Home Manager module.
- The module can consume Stylix configuration.
- Supported colours are correctly mapped into the application theme.
- Supported font settings are applied.
- User overrides work predictably.
- The integration is optional.
- Both standalone Home Manager and NixOS/Home Manager usage are documented and tested.
- Existing theme configuration remains compatible.
- The Rust application remains independent of Stylix at runtime.

## 15. Implementation phases

**Phase 1 — Repository and architecture investigation**

Inspect the existing flake, Home Manager integration, theme configuration and Rust theme architecture.

Identify the appropriate integration points and any missing theme capabilities.

**Phase 2 — Module design**

Define the Home Manager module interface, option names, colour mappings, font mappings and precedence rules.

Present the design for review before implementation.

**Phase 3 — Initial integration**

Implement the basic module, package integration and colour mapping.

**Phase 4 — Fonts and additional supported properties**

Implement font mapping and any additional theme properties justified by the application's existing capabilities.

**Phase 5 — Tests and documentation**

Add Nix evaluation tests, build tests, visual verification and usage documentation.

**Phase 6 — Final verification**

Test standalone Home Manager and NixOS with Home Manager, verify that existing configurations still work and confirm that the application has no Stylix runtime dependency.

## 16. Instructions for Claude Code

Read this specification alongside:

- `CLAUDE.md`
- The original product specification.
- The spatial file manager specification.
- Existing architecture documentation and ADRs.
- Existing themeability requirements.
- Current Nix flake and packaging implementation.

Do not assume the application uses GTK, Qt or any other particular UI framework. Inspect the actual implementation.

Do not weaken existing Linux, native Wayland, Clean Architecture or themeability requirements.

Do not introduce a parallel theme engine.

Do not implement anything immediately.

First inspect the repository and produce:
1. A summary of the existing Nix and theme architecture.
2. A proposed Home Manager module design.
3. A proposed Stylix-to-application colour mapping.
4. A proposed font mapping.
5. Any required changes to the Rust theme system.
6. A list of potential compatibility issues.
7. A phased implementation and testing plan.

Identify ambiguities and recommend resolutions rather than silently making assumptions.

Wait for approval before modifying source code or configuration.

## 17. Definition of done

The file manager has a documented, optional and reusable Home Manager module that integrates with Stylix, applies supported theme settings correctly, respects user overrides and preserves the application's existing theme architecture.

Users should be able to enable the file manager in their Nix configuration and have it automatically adopt their Stylix theme without manually duplicating colour and font settings.
