# PR-97 - Real Terminal, SSH, and TN3270 Replay and Recording

## Goal

Replace the in-memory terminal adapter with real terminal session ownership for replay and recording.

## User Outcome

Greentic can open a controlled terminal, SSH, or TN3270 session, send input, wait for screen state, extract fields, and record terminal interactions from owned sessions.

## Current Evidence

- `TerminalAdapter::execute` mutates `TerminalState`.
- Recording requires `GREENTIC_TERMINAL_RECORDER_COMMAND`; otherwise it blocks.

## Scope

1. Add owned PTY runtime for local shell using a proven crate such as `portable-pty`.
2. Add SSH runtime using a maintained SSH crate or system `ssh` process under controlled PTY.
3. Add TN3270 runtime or integrate an existing emulator library/process with screen buffer access.
4. Implement:
   - `terminal.connect`
   - `terminal.disconnect`
   - `terminal.type_text`
   - `terminal.send_text`
   - `terminal.send_keys`
   - `terminal.read_screen`
   - `terminal.wait_for_screen`
   - `terminal.extract_field`
   - `terminal.assert_text`
   - `terminal.capture_screen`
5. Replace environment-command recording with first-class owned session event capture.
6. Persist terminal buffer snapshots as evidence.
7. Add timeouts and redaction for secrets typed into terminals.

## E2E Fixtures

1. Local PTY fixture running a small interactive script.
2. Optional containerized SSH fixture.
3. TN3270 fixture/emulator if available; otherwise contract test remains blocked with explicit capability unavailable.

## Acceptance Tests

1. Local PTY runner enters values and extracts real screen output.
2. Recording an owned PTY session captures typed commands and outputs.
3. Existing arbitrary terminal windows remain unsupported with clear message.
4. Secret inputs are redacted from logs and evidence.
5. Terminal runner fails on timeout rather than passing with stale screen content.

