# PR-131 - Windows Adapter Use windows-rs UIA And Enigo SendInput

## Goal

Implement Windows replay and recording on top of UI Automation and real input synthesis instead of PowerShell/string-rendered scripts.

## User Outcome

Windows native applications can be targeted generically by UI Automation metadata with reliable diagnostics and real side-effect proof.

## Current Evidence

- Windows adapter has useful generated UIA script tests, but production behavior should use `windows` or `uiautomation` crate APIs directly.
- Generic document/spreadsheet/app workflows need element-targeted automation, not generated PowerShell snippets.

## Scope

1. Add dependencies:
   - `windows` for official Windows APIs.
   - evaluate `uiautomation` crate for higher-level UIA traversal and actions.
   - `enigo` or `windows` `SendInput` for fallback input synthesis.
   - `xcap` for screenshots through foundation.
2. Implement UIA tree traversal:
   - AutomationId.
   - Name.
   - ControlType.
   - ClassName.
   - BoundingRectangle.
   - Value/Invoke/Text patterns.
3. Implement generic primitives:
   - open/activate app.
   - find/focus element.
   - invoke/click element.
   - set value/type text.
   - press shortcut.
   - read text/value.
   - save/open dialog handling.
4. Add recording preflight for event source availability and elevated-target restrictions.
5. Use shared screenshot and redacted diagnostics.

## File Targets

- `crates/greentic-desktop-windows/src/lib.rs`
- `crates/greentic-desktop-platform/src/lib.rs`
- `crates/greentic-desktop-workflow/src/lib.rs`
- `docs/adapters/windows-ui.md`
- `docs/capability-matrix.md`

## Out of Scope

- Automating Office through COM-specific APIs.
- App-specific document hardcoding.

## Acceptance Tests

1. Windows adapter core replay primitives use UIA crate APIs, not generated PowerShell.
2. UIA locator tests cover AutomationId, Name, ControlType, ClassName, and bounds.
3. Elevated target and missing desktop session failures are explicit.
4. Real Windows fixture E2E opens a simple native fixture app, writes data, saves a file, verifies the file exists, and returns output.
5. The adapter never claims success when the output artifact does not exist.

## Done Means

Windows native automation uses real UI Automation APIs and real input, not script string generation.
