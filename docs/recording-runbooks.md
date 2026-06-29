# Recording Runbooks

Greentic Desktop records controlled automation sessions. It does not silently record arbitrary tabs, unmanaged terminal windows, or unrelated desktop apps. A recording is either attached to an active backend with a heartbeat, observe-only with an explicit limitation, or blocked with a concrete reason.

For replay readiness, output proof, and the complete target support matrix, see [Production Readiness Matrix](production-readiness.md).

## Target Matrix

| Target | What is recorded first | Required setup | Fast automated coverage |
| --- | --- | --- | --- |
| Web | Greentic-owned browser context opened for recording. Existing browser tabs are not captured yet. | Playwright/web adapter available. | `cargo test -p greentic-desktop-web` and `cargo test -p greentic-desktop-test-harness recording_e2e_matrix_normalises_semantic_runners_without_placeholders` |
| Native desktop | Platform accessibility events, focused windows, typed text, clicks, UI tree/screenshot evidence. | macOS Accessibility and Screen Recording; Windows same-user UIA; Linux X11 AT-SPI/XTest; Wayland portal-limited support. | `cargo test -p greentic-desktop-macos`, `-p greentic-desktop-windows`, `-p greentic-desktop-linux` |
| Java | Java Access Bridge/Swing/AWT component events and component tree snapshots. | Java accessibility bridge available for the target environment. | `cargo test -p greentic-desktop-java` |
| Terminal/mainframe | Greentic-owned PTY, SSH, or TN3270 session input and screen buffers. Existing terminal windows are not captured yet. | Terminal profile owned by Greentic; credentials stored as secrets, not typed into runner YAML. | `cargo test -p greentic-desktop-terminal` |
| Remote desktop | Greentic-owned remote viewport screenshots, OCR/vision observations, calibrated coordinates, and input events. | Screen capture, keyboard/mouse control, and viewport calibration. | `cargo test -p greentic-desktop-vision` |

The combined recording matrix is in `greentic-desktop-test-harness`. It fails if normalized runners contain placeholders such as `sample-output`, `recording.recorded`, or CRM defaults in non-CRM fixtures.

## How Recording Starts

1. The GUI or CLI creates a recording session manifest.
2. The selected target is matched to a `RecordingBackend`.
3. The backend runs preflight checks for ownership, permissions, and target limitations.
4. If preflight passes, the backend appends `recording.event.v1` events to `raw/events.jsonl` and updates heartbeat metadata.
5. If preflight fails, the session is `blocked`; normalisation is disabled until there are real captured events or explicit manual markers.

Raw events use this envelope:

```json
{
  "schema_version": "recording.event.v1",
  "session_id": "rec_...",
  "backend": "greentic.recording.web.playwright",
  "target_kind": "web",
  "timestamp": "2026-06-27T10:00:00Z",
  "sequence": 1,
  "event": {
    "kind": "click",
    "target": {},
    "value": null,
    "redaction": "none"
  },
  "evidence": {
    "screenshot_ref": null,
    "dom_snapshot_ref": null,
    "ui_tree_ref": null,
    "terminal_buffer_ref": null
  }
}
```

## Local Test Commands

Run the fast recording checks:

```bash
cargo test -p greentic-desktop-recorder
cargo test -p greentic-desktop-test-harness recording_e2e_matrix_normalises_semantic_runners_without_placeholders
npm --prefix frontend/automate-hub run typecheck
```

Run target-specific checks:

```bash
cargo test -p greentic-desktop-web
cargo test -p greentic-desktop-macos
cargo test -p greentic-desktop-windows
cargo test -p greentic-desktop-linux
cargo test -p greentic-desktop-java
cargo test -p greentic-desktop-terminal
cargo test -p greentic-desktop-vision
```

## Troubleshooting

### Browser

Greentic opens a controlled browser context. Actions in unrelated Chrome, Safari, Firefox, or Edge tabs are outside the recorded context. If browser recording is blocked, verify the Playwright/web adapter is installed and start recording from the Browser task target.

### macOS

When launched with `cargo run` from Terminal, iTerm2, VS Code, or Cursor, macOS permissions usually apply to that launcher process, not a packaged Greentic app. Grant Accessibility and Screen Recording to the launcher or to the debug binary path under `target/debug/greentic-desktop`, then restart if macOS asks.

### Windows

Same-user UI Automation generally works without extra permission. Recording an elevated application requires Greentic Desktop to run elevated too. If a session is blocked for elevation, restart Greentic Desktop with matching integrity level or record a non-elevated target.

### Linux

X11 recording expects AT-SPI/window metadata and screenshot support. Wayland blocks global input/window capture when the compositor forbids it; portal screenshots and safe shortcuts may still be available, but unsupported global capture must remain explicit.

### Java

Java recording requires Java Access Bridge or equivalent accessibility support. If preflight blocks, enable Java accessibility for the JDK/runtime that launches the app and restart the app before recording.

### Terminal/Mainframe

Recording only captures Greentic-owned PTY, SSH, or TN3270 sessions. Existing Terminal/iTerm tabs are not recorded yet. Store passwords in secrets/profile configuration; password prompt input is redacted in raw events.

### Remote Desktop

Remote recording requires a Greentic-owned viewport, screen capture, input control, and calibration. If the viewport cannot be calibrated, select or launch the remote session from Greentic and retry after granting screen and input permissions.
