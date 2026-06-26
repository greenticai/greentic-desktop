# PR-65 - Web, Terminal, and Remote Recording Backends

## Goal

Implement real recorders for browser automation, terminal/mainframe sessions, and remote desktop sessions using proven existing libraries and protocols.

## Problem

The current web, terminal, and remote recording surfaces are generic adapter simulations. They can replay accepted steps in memory, but they do not capture a human session into reusable runner steps.

## User Outcome

Users can record high-value automations without relying on fragile desktop-wide input capture when better structured technology-specific sources exist:

- Web: capture DOM-backed clicks, fills, navigation, network/download events, and extractors.
- Terminal/mainframe: capture command/input/output transcripts and screen buffer changes.
- Remote desktop: capture screenshots and input at the remote session boundary with explicit safety constraints.

## Web Recorder

Reuse Playwright instead of building a custom browser recorder.

Approach:

- Use Playwright codegen/tracing concepts for event capture.
- Where the GUI browser extension is installed, capture events through the extension content script and DevTools protocol.
- Normalize events to generic `RawRecordingEvent`:
  - `browser_navigation`
  - `click`
  - `text_committed`
  - `select_changed`
  - `file_uploaded`
  - `download_started/completed`
  - `network_error`
  - `console_error`
  - `output_observed`
- Generate stable selectors using Playwright locator priority:
  - role/name
  - label
  - test id
  - text
  - CSS only as fallback
  - XPath only as last resort

Implementation details:

- Add a recorder implementation to `crates/greentic-desktop-web`.
- Keep browser-specific transport behind a trait so the GUI extension and Playwright-controlled browser can both feed the same event model.
- Store Playwright trace artifacts as evidence when available.
- Use screenshots and accessibility snapshots to create robust fallbacks.

## Terminal/Mainframe Recorder

Reuse PTY and terminal parsing libraries instead of manually parsing terminal escape sequences.

Approach:

- For local shell recording, wrap the launched process in a pseudo-terminal.
- Capture:
  - command input
  - submitted Enter events
  - stdout/stderr output
  - terminal screen buffer snapshots
  - exit status
  - prompts and detected output values
- For TN3270/mainframe, record screen buffer states and field edits from the terminal adapter boundary.

Recommended libraries:

- `portable-pty` for cross-platform PTY sessions.
- `vt100` or an equivalent terminal parser for screen buffer snapshots.
- Existing TN3270 adapter protocol parsing where available.

Implementation details:

- Add `RecordingBackend` implementation in `crates/greentic-desktop-terminal`.
- Redact commands/arguments using configured redaction rules before persistence.
- Produce output extractors from terminal regions, regexes, and final command output.

## Remote Desktop Recorder

Remote desktop should record at the session boundary, not by pretending the local OS knows the remote app.

Approach:

- Use the existing screen/input backend for screenshots and input.
- Attach remote session metadata:
  - provider
  - workspace/session ID
  - display geometry
  - scaling
  - focused region
- Emit visual locator events with screenshot regions and optional OCR text.
- Do not store raw remote screenshots outside the evidence store.

Recommended libraries:

- Reuse existing screenshot/input abstraction from `greentic-desktop-io`.
- Use OCR only through an explicit vision/OCR adapter; do not bake OCR into the recorder runtime.

## Acceptance Criteria

- Web recording captures a real click/fill/navigation flow and normalizes to semantic web runner steps.
- Terminal recording captures a command, its output, and at least one output extractor.
- Remote recording captures screenshots, input events, and visual locator metadata with clear confidence scores.
- All three backends plug into the PR-63 recorder runtime.
- Web and terminal backends work without global OS input monitoring permissions.

## Test Plan

- Web integration test with a local test page and Playwright-backed event feed.
- Terminal integration test with a simple command that prints deterministic output.
- Remote recorder unit test with fake screenshot/input events.
- Regression tests for redaction in web form fields and terminal command arguments.

## Risks

- Browser extension and Playwright-controlled browser event formats can drift. Keep a typed internal event schema and conversion tests.
- Terminal recording can capture secrets in commands or output. Redaction must run before raw persistence.
- Remote visual recording is inherently less stable than semantic app recording and must carry confidence metadata.

