# PR-130 - macOS Adapter Use objc2 AX CoreGraphics And Xcap

## Goal

Replace macOS `osascript`, `open`, and `screencapture` scripting with native Rust bindings and shared foundation backends.

## User Outcome

macOS native app replay and recording operate through Accessibility APIs, CoreGraphics input, and real screenshot capture instead of brittle AppleScript strings.

## Current Evidence

- macOS adapter currently has generic action coverage but still leans on scripting/process calls for app activation and UI actions.
- Reliable element-targeted automation requires the AX element tree, not blind keyboard shortcuts and AppleScript.

## Scope

1. Add dependencies:
   - `objc2`
   - `objc2-foundation`
   - `objc2-app-kit`
   - AX bindings through `accessibility` / `accessibility-sys` or direct AX FFI if no maintained crate is adequate.
   - `core-graphics`
   - `xcap` through the foundation screenshot backend.
2. Implement AX tree traversal:
   - roles.
   - titles.
   - identifiers.
   - values.
   - enabled/focused state.
   - bounds.
3. Implement generic actions:
   - activate app.
   - focus window/document/control.
   - click element.
   - type text.
   - press shortcut.
   - invoke menu.
   - save/open dialog handling.
   - read text/value.
4. Use `core-graphics` or `enigo` for input synthesis where AX action is insufficient.
5. Use `xcap` for screenshots/evidence.
6. Add permission preflight that checks Accessibility, Input Monitoring, and Screen Recording with exact user guidance.

## File Targets

- `crates/greentic-desktop-macos/src/lib.rs`
- `crates/greentic-desktop-platform/src/lib.rs`
- `crates/greentic-desktop-workflow/src/lib.rs`
- `docs/adapters/macos-accessibility.md`
- `docs/capability-matrix.md`

## Out of Scope

- App-specific Word/Excel hardcoding.
- Java Access Bridge implementation on macOS; Java apps should surface through AX where possible.

## Acceptance Tests

1. macOS adapter no longer shells out to `osascript` for core replay primitives.
2. AX locator tests cover role/title/identifier/value/bounds matching.
3. Permission preflight returns concrete missing permission names.
4. A real fixture app E2E opens a simple native test app, writes data, saves a file, verifies the file exists, and returns output.
5. Failure diagnostics identify the exact step, locator, and AX/action failure.

## Done Means

macOS is driven by native accessibility and input APIs, not AppleScript automation.
