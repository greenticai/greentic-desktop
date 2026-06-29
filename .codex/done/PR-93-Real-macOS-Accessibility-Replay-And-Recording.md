# PR-93 - Real macOS Accessibility Replay and Recording

## Goal

Replace the in-memory macOS adapter with real macOS Accessibility, input event, screen capture, and recording implementations.

## User Outcome

On macOS, Greentic can open visible desktop apps, locate windows/controls through Accessibility, type/click using permitted input APIs, read UI text, save files when the workflow asks for it, and record real user actions.

## Current Evidence

- `MacOsAccessibilityAdapter::execute` mutates in-memory state and returns `"macOS AX step accepted"`.
- `MacOsAccessibilityRecordingBackend::start` emits a synthetic focused-app event.
- Permission checks exist but do not imply real execution.

## Scope

1. Add macOS native implementation using proven crates or Objective-C/Swift bridge:
   - `accessibility`/AXUIElement APIs for tree traversal.
   - `core-graphics`/Quartz Event Services for input events.
   - `screencapture`/ScreenCaptureKit or CGWindow capture for screenshots where allowed.
2. Implement:
   - `macos.find_app`
   - `macos.activate_app`
   - `macos.find_window`
   - `macos.read_window_tree`
   - `macos.find_element`
   - `macos.type_text`
   - `macos.click_element`
   - `macos.read_text`
   - `macos.assert_visible`
   - `macos.screenshot`
   - `macos.close_app`
3. Add app launcher abstraction using LaunchServices/open, not shell strings.
4. Implement event tap recording for:
   - focused app/window changes
   - mouse clicks
   - keyboard text
   - UI element metadata under pointer/focus
5. Add permission diagnostics that distinguish:
   - Accessibility
   - Input Monitoring
   - Screen Recording
   - app bundle/terminal identity
6. Persist AX tree snapshots and screenshots as evidence.
7. Remove synthetic recording events from production macOS backend.

## E2E Fixtures

1. Local SwiftUI/AppKit fixture app with text fields, buttons, table, save-to-file flow.
2. Calculator smoke test using generic action sequence, not calculator-specific code.
3. TextEdit or fixture document save test that verifies a real output file exists.

## Acceptance Tests

1. With permissions missing, adapter advertises no executable macOS capabilities and setup explains the exact app to approve.
2. With permissions granted, the fixture app E2E creates a real file and output verification passes.
3. Recording real clicks/types in the fixture app produces a replayable runner.
4. Replay visibly drives the app; no in-memory state is used for success.
5. CI includes unit/contract tests; macOS E2E runs on macOS runner with permission-aware skips only when OS policy prevents approval automation.

