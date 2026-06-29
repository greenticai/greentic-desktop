# PR-94 - Real Windows UI Automation Replay and Recording

## Goal

Replace the in-memory Windows adapter with real Windows UI Automation, input, screenshot, and event recording.

## User Outcome

On Windows, Greentic can drive real desktop apps through UIA patterns, create/edit/save documents, extract UI text, and record real user interactions.

## Current Evidence

- `WindowsUiAdapter::execute` mutates `WindowsState` and returns `"windows step accepted"`.
- `WindowsUiRecordingBackend::start` emits a synthetic focused-window event.

## Scope

1. Implement Windows adapter using `windows` crate:
   - UIAutomationCore
   - `IUIAutomationElement`
   - Invoke/Value/Selection/Text patterns
   - Win32 input APIs as fallback where UIA pattern is unavailable
2. Implement:
   - `windows.open_app`
   - `windows.find_window`
   - `windows.read_window_tree`
   - `windows.find_element`
   - `windows.type_text`
   - `windows.click_element`
   - `windows.read_text`
   - `windows.assert_visible`
   - `windows.screenshot`
   - `windows.close_app`
3. Detect elevated-target mismatch and fail before execution unless Greentic is elevated.
4. Implement UIA event subscriptions for recording:
   - focus changed
   - structure changed
   - invoke
   - value changed
5. Persist UIA tree snapshots and screenshots.
6. Remove synthetic recording events from production Windows backend.

## E2E Fixtures

1. WinUI/WPF fixture app with text fields, save button, output label, and document path save flow.
2. Notepad smoke test only as a secondary real-app check.

## Acceptance Tests

1. Fixture runner creates a real file at a user-provided path.
2. Missing/elevated permission mismatch blocks with actionable diagnostic.
3. Recording fixture interactions produces a replayable generic workflow.
4. UIA unavailable controls fall back to input only when a target window/control is safely resolved.
5. No test relies on in-memory `WindowsState` for product execution.

