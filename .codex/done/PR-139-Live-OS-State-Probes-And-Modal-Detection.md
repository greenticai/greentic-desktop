# PR-139 - Live OS State Probes And Modal Detection

## Goal

Create shared live desktop probes that inspect real OS state after every workflow step: foreground app, windows, modal dialogs, screenshots, file changes, and accessibility availability.

## User Outcome

When automation fails, Greentic tells the user what is actually on screen, which dialog is blocking progress, and which buttons are available.

## Current Evidence

- Excel displayed stacked replace/save dialogs that were not clicked and were hard to diagnose from runner output.
- AppleScript/Accessibility failures currently collapse into generic `step failed` messages.
- The product needs to validate real desktop state without asking the user to provide screenshots manually.

## Scope

1. Add a `LiveDesktopProbe` trait:
   - `snapshot() -> LiveDesktopSnapshot`
   - `frontmost_app()`
   - `list_windows()`
   - `list_modals()`
   - `capture_screenshot()`
   - `file_state(path)`
2. Add typed snapshot models:
   - app name.
   - window title.
   - role/subrole/control type.
   - modal text.
   - button labels.
   - focused element summary.
   - screenshot reference.
3. Implement macOS probe using the real Accessibility API path already available to the adapter, not a separate mock path.
4. Add Windows probe design using UI Automation.
5. Add Linux probe design:
   - X11 via AT-SPI/X11 state where available.
   - Wayland via portals where available.
   - explicit unsupported status when the session cannot expose global windows.
6. Add redaction:
   - redact user home path segments in user-facing logs.
   - keep full paths in local evidence only when marked local/private.
7. Add modal classification:
   - confirmation.
   - permission.
   - file overwrite.
   - file format conversion.
   - unsaved changes.
   - unknown blocking modal.

## File Targets

- `crates/greentic-desktop-runtime/src/live_probe.rs`
- `crates/greentic-desktop-macos/src/lib.rs`
- `crates/greentic-desktop-windows/src/lib.rs`
- `crates/greentic-desktop-linux/src/lib.rs`
- `crates/greentic-desktop-replay/src/lib.rs`

## Out of Scope

- Solving every modal automatically.
- Recording full screen video.

## Acceptance Tests

1. macOS probe reports a blocking Excel replace dialog with text and buttons when that dialog is open.
2. macOS probe reports no modal for a normal workbook/document window.
3. File state captures existence, size, and modified timestamp.
4. Probe snapshots are included in failed replay evidence.
5. Unsupported Linux Wayland state is explicit and does not pretend validation passed.

## Done Means

Every live workflow failure can include a faithful desktop snapshot, so debugging does not depend on the user sending screenshots.
