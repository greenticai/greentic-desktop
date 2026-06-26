# PR-64 - Native Desktop Recording Backends for macOS, Windows, and Linux

## Goal

Implement real native desktop recording backends that capture user actions and UI state on macOS, Windows, Linux X11, and Linux Wayland where the OS allows it.

## Problem

The current platform adapters simulate or remember actions executed by Greentic. They do not observe a human using arbitrary desktop applications.

Examples:

- macOS stores events when `execute` is called, not when the user clicks or types.
- Windows has `record_control_interaction`, but nothing hooks UI Automation events globally.
- Linux X11 and Wayland adapters record Greentic-executed steps, not human input.
- Screen capture permissions are probed, but screenshots are not written as recording evidence.

## User Outcome

A user can record a native desktop app and see real captured actions:

- app/window activation
- focused element
- click target
- typed text, redacted where needed
- selected menu/button/control
- before/after screenshots
- output observations

The same recording model works across OSs, with platform-specific locator candidates.

## macOS Backend

Use native macOS APIs instead of hand-rolled screen scraping:

- Accessibility API (`AXUIElement`) for focused app/window/element, roles, titles, values, labels, and actions.
- Accessibility notifications where practical:
  - focused UI element changed
  - focused window changed
  - selected text changed
  - value changed
- CoreGraphics event taps for mouse/keyboard capture when Input Monitoring is granted.
- ScreenCaptureKit or CoreGraphics screenshots for evidence. Prefer ScreenCaptureKit on modern macOS for reliability.

Implementation notes:

- Add a small `greentic-desktop-macos-recorder` module behind `cfg(target_os = "macos")`.
- Use `objc2`, `core-foundation`, `core-graphics`, and `accessibility-sys`/AX FFI rather than shelling out to AppleScript.
- Store bundle ID, process ID, app name, window title, AX role, AX subrole, AX identifier where available, title, description, value hash, and bounding rectangle.
- Never persist raw password fields. If AX role/subrole indicates secure text, store a secret marker only.

## Windows Backend

Use Windows UI Automation and low-level input hooks:

- UI Automation COM event handlers for focus, invoke, value, text, and structure changes.
- `SetWinEventHook` for foreground/window events.
- Raw input or low-level keyboard/mouse hooks only for timing and fallback, not as primary locators.
- Windows Graphics Capture or existing screenshot backend for evidence.

Implementation notes:

- Add backend under `crates/greentic-desktop-windows` with `cfg(target_os = "windows")`.
- Use the `windows` crate for UI Automation, WinEvent hooks, window metadata, and screenshots.
- Capture stable locator candidates:
  - AutomationId
  - Name
  - ControlType
  - ClassName
  - process executable
  - window title
  - bounding rectangle
- Deduplicate hook events with UI Automation events by sequence/time/window/control.

## Linux X11 Backend

Use the established Linux desktop accessibility and input APIs:

- AT-SPI via `atspi`/D-Bus for accessible tree, focus, object events, names, roles, values, and actions.
- X11 Record/XInput2 or XTest-compatible capture for mouse/keyboard fallback.
- X11 window metadata through XCB/Xlib where needed.
- Screenshot through the existing `greentic-desktop-io` backend or `xcap` if it proves more reliable.

Implementation notes:

- Prefer AT-SPI semantic events over raw coordinates.
- Use raw X events only to associate a click with the current accessible element and screenshot region.
- Capture desktop file/app ID where available.

## Linux Wayland Backend

Wayland intentionally restricts global input capture. The backend must be honest:

- Use AT-SPI for accessible app events when available.
- Use xdg-desktop-portal for screenshots/screencast with explicit user approval.
- Do not promise global key/mouse capture where the compositor blocks it.
- For unsupported global recording, return `capture_state = blocked` with required manual alternatives:
  - browser recorder
  - terminal recorder
  - app-specific accessibility recorder
  - remote controlled session

Implementation notes:

- Use `ashpd` for portal screenshot/screencast flows if adding a crate is acceptable.
- Use D-Bus/AT-SPI for semantic events.
- Store compositor/session diagnostics in `captureBlockedReasons`.

## Shared Native Event Shape

Each native backend should emit:

- `app_activated`
- `window_focused`
- `element_focused`
- `click`
- `text_committed`
- `key_sequence`
- `value_changed`
- `screenshot_captured`
- `output_observed`

Coordinates are evidence only. Replay must prefer accessibility locators.

## Acceptance Criteria

- macOS records at least Calculator/TextEdit interactions into real event JSONL.
- Windows records at least Calculator/Notepad interactions into real event JSONL.
- Linux X11 records at least a GTK or Qt app through AT-SPI plus screenshot evidence.
- Linux Wayland reports exact supported/blocked capabilities and records AT-SPI events when available.
- Sensitive text fields are redacted before persistence.
- Platform tests use fake event sources for CI plus optional ignored/manual tests for real OS permissions.

## Test Plan

- Unit tests for locator extraction and redaction per OS.
- Fake event-source tests for each backend.
- Manual macOS test: grant Accessibility, Screen Recording, Input Monitoring; record a Calculator addition.
- Manual Windows test: record Notepad text entry and Calculator addition.
- Manual Linux X11 test under `xvfb-run` where possible and full desktop manual test for AT-SPI.
- Manual Wayland test verifies blocked state and portal screenshot path.

## Risks

- Native event APIs differ heavily by OS and desktop environment.
- macOS event taps require the process that owns the binary to be permissioned, which must be clear in GUI setup.
- Wayland cannot support global recording without compositor/user approval; the product must communicate that constraint directly.

