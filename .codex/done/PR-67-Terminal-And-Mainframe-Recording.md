# PR-67 - Terminal and Mainframe Recording

## Goal

Make terminal/mainframe recording real by capturing terminal input and screen buffer transitions, then normalizing them into replayable terminal workflows.

## Problem

The current terminal path can model terminal actions, but recording does not own a terminal session or capture typed commands, function keys, screen buffers, cursor positions, or extracted fields.

## Scope

1. Add a terminal recording backend plugged into PR-63.
2. Greentic must own the terminal connection during recording:
   - local PTY shell
   - SSH where configured
   - TN3270/mainframe profile where configured
3. Capture:
   - typed text
   - enter/function keys
   - cursor position
   - terminal screen buffer before/after actions
   - prompt detection
   - field positions for 3270 screens
4. Redact secrets:
   - password prompts
   - hidden input mode
   - configured secret markers
5. Normalize events to:
   - `terminal.connect`
   - `terminal.send_text`
   - `terminal.send_keys`
   - `terminal.wait_for_screen`
   - `terminal.extract_field`
   - `terminal.assert_text`
   - `terminal.disconnect`

## Ownership Rules

Initial implementation records only Greentic-owned terminal sessions. It does not claim to record arbitrary Terminal/iTerm tabs.

The UI must say:

- "Greentic will open/connect a terminal session for recording."
- "Existing terminal windows are not recorded yet."

## Fixture Targets

Add deterministic test fixtures:

- local pseudo-terminal script that asks for two numbers and operation
- TN3270-like screen fixture or emulator stub
- password prompt fixture for redaction

## Acceptance Criteria

- Recording the local terminal calculator fixture produces a runner that replays and returns `2`.
- Screen buffer snapshots are stored as evidence.
- Password prompt input is redacted.
- Recording existing unmanaged terminal windows is blocked honestly.
- Runner output extractors read from terminal buffer/field positions, not hardcoded strings.

## Test Plan

- Unit tests for terminal event envelope.
- PTY integration test for command/input capture.
- TN3270 buffer parser test.
- Normalization test from terminal JSONL to runner YAML.
- Replay test against local fixture script.

## Done Means

"Terminal/mainframe task" records a real Greentic-owned terminal session and produces replayable terminal runners.

