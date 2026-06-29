# PR-112 - App Profile Manifests for Generic Primitive Lowering

## Goal

Add optional data-driven app profiles that help lower generic primitives without hardcoding app behavior in Rust.

## User Outcome

Common apps can become reliable faster, while the core remains generic and extensible.

## Current Evidence

- Some apps expose document body, menu, save dialog, and tables differently.
- Hardcoding Word or Excel in Rust would violate the product goal.

## Scope

1. Define app profile manifest format:
   - app aliases
   - platform identifiers
   - resource types supported
   - menu paths
   - dialog target hints
   - accessibility target hints
   - keyboard fallback policy
2. Add profile resolver:
   - match by app name/bundle id/process id/window title.
3. Add built-in generic profiles:
   - generic document editor
   - generic spreadsheet editor
   - generic browser
   - generic terminal
4. Allow extension packages to contribute profiles.
5. Ensure profiles are declarative and never contain secrets.

## Out of Scope

- App-specific Rust branches.
- Marketplace distribution.

## Acceptance Tests

1. A profile can map `document_body` to accessibility hints and menu paths.
2. Word can be supported via manifest data, not Rust `if app == "Word"`.
3. LibreOffice/TextEdit can use the same primitive workflow with different profile data.
4. Missing profile falls back to generic OS lowering with clear confidence warnings.
5. Invalid profile manifests are rejected at install/verify time.

