# PR-65 - Native Desktop App Recording on macOS, Windows, and Linux

## Goal

Make "Desktop app task" recording real for native desktop applications through platform accessibility APIs, with honest permission handling and cross-platform event normalization.

## Problem

The current native desktop recording flow creates a session but does not observe user interactions in another app. It also blurs three different requirements: screen capture, accessibility tree observation, and keyboard/mouse control.

## Scope

Implement native desktop recording backends:

- macOS Accessibility + Screen Recording
- Windows UI Automation
- Linux X11 AT-SPI/XTest where available
- Linux Wayland portal-limited observation with explicit limitations

Each backend must plug into PR-63's `RecordingBackend` and emit `recording.event.v1`.

## Event Capture Requirements

Capture:

- focused app/window changes
- accessibility focused element
- button/menu clicks
- text entry into accessible fields
- keyboard shortcuts
- selection changes
- visible text snapshots
- screenshots where permission allows
- UI tree snapshots around events

## Permission Rules

### macOS

Required:

- Accessibility for UI tree and action observation
- Screen Recording for screenshots/visual fallback
- Input Monitoring only if low-level key capture is used

The UI must explain that `cargo run` users may need to grant permission to Terminal, iTerm, VS Code, Cursor, or the debug binary path.

### Windows

Required:

- UI Automation generally no special permission for same-user desktop
- elevated app recording requires Greentic to run elevated or clearly block

### Linux X11

Required:

- AT-SPI bus available
- X11 session for global window/input observation
- XTest if low-level input replay is needed

### Linux Wayland

Required:

- xdg-desktop-portal screen capture approval for screenshots
- accessibility APIs where desktop environment exposes them

Wayland must not pretend to support global input/window capture where the compositor forbids it.

## Normalized Actions

Native recordings should produce semantic actions:

- `desktop.open_app`
- `desktop.activate_window`
- `desktop.click`
- `desktop.type_text`
- `desktop.select_menu`
- `desktop.press_shortcut`
- `desktop.read_text`
- `desktop.extract_field`
- `desktop.assert_text`

Adapter compilation can map these to macOS/windows/linux-specific capabilities.

## Acceptance Criteria

- macOS Calculator fixture can be recorded into a runner that replays and returns `2` for `1 + 1`.
- Windows Calculator fixture can be recorded into a runner that replays and returns `2` for `1 + 1`.
- Linux X11 sample app fixture can be recorded into a runner that replays and returns expected output.
- Wayland blocked/limited states are explicit and tested.
- Missing permissions block recording with specific instructions; they do not create empty recordings.
- Raw events include UI tree or screenshot evidence references where available.

## Test Plan

- macOS manual/E2E: Calculator recording with permissions.
- Windows CI/manual: Calculator recording through UIA.
- Linux X11 CI with Xvfb: sample GTK/HTML app recording through AT-SPI.
- Wayland unit tests for portal-required blocked state.
- Backend fake accessibility tree tests for normalization.

## Done Means

"Desktop app task" either records real native app interactions or clearly blocks with missing capability/permission. Empty placeholder sessions are not acceptable.

