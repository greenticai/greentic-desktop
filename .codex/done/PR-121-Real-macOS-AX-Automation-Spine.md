# PR-121 - Real macOS AX Automation Spine

## Goal

Move macOS automation from brittle AppleScript-style shims toward a real Accessibility API automation spine.

## User Outcome

Generic desktop actions such as activate app, locate focused document, type text, press keyboard shortcuts, click named controls, save resource, and read text behave predictably for real macOS apps where permissions allow it.

## Current Evidence

- `macos/src/lib.rs` uses `osascript`, `open`, and `screencapture`.
- Generated `Cmd+N` command currently lowers to `macos.click_element` with an empty locator.
- Failure diagnostics only recently became useful, but primitives still lower to weak macOS steps.

## Scope

1. Add explicit macOS capabilities:
   - `macos.press_shortcut`
   - `macos.invoke_menu`
   - `macos.focus_document`
   - `macos.save_as`
2. Lower primitives correctly:
   - `InvokeCommand { shortcut: "Cmd+N" }` -> `macos.press_shortcut`.
   - `SaveResource { path_template }` -> `macos.save_as` or a documented fallback.
3. Implement AX-backed sidecar:
   - Swift or Rust objc/core-foundation code compiled as sidecar.
   - communicate JSON over stdio.
   - no runtime source injection.
4. Keep AppleScript only as explicit fallback with lower confidence.
5. Add fixture app:
   - a tiny native macOS test app or TextEdit-compatible fixture.
   - E2E: open app, type text, save to temp path, verify file exists.
6. Permissions:
   - preflight detects Accessibility/Input Monitoring/Screen Recording.
   - error tells user exactly which launcher needs permission.

## File Targets

- `crates/greentic-desktop-macos/src/lib.rs`
- `crates/greentic-desktop-workflow/src/lib.rs`
- `crates/greentic-desktop-platform/src/lib.rs`
- `crates/greentic-desktop-test-harness/src/lib.rs`
- `extensions/*` or sidecar packaging path.

## Out of Scope

- Perfect support for every macOS app.
- OCR fallback improvements.

## Acceptance Tests

1. `Cmd+N` primitive compiles to `macos.press_shortcut`, not click.
2. Empty locators are rejected before execution.
3. Missing permissions produce actionable failure text.
4. Fixture app E2E creates a real file in `/tmp` and verifies it exists.
5. The same runner returns failed-step diagnostics when the app is missing.

## Done Means

The Word-style prompt fails or succeeds for concrete platform reasons, not because primitives lower to nonsensical click steps.
