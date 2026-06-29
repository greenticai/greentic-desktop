# PR-100 - Real Recording Capture Streams for All Targets

## Goal

Replace synthetic initial recording events with continuous real event streams for web, desktop, Java, terminal, remote, and vision-backed sessions.

## User Outcome

When the user starts recording and performs actions, Greentic captures those actions, derives inputs/secrets/outputs/assertions, and creates a runner that can replay the demonstrated workflow.

## Current Evidence

- macOS, Windows, Linux, Java, and remote recording `start()` methods emit synthetic focused events.
- Terminal recording depends on external command configuration.
- Web recording is closest but still scoped to Greentic-owned contexts.

## Scope

1. Introduce `RecordingEventStream` abstraction with heartbeat, shutdown, and error reporting.
2. Implement real stream per target:
   - web Playwright/CDP events
   - macOS AX/event tap
   - Windows UIA events
   - Linux AT-SPI/X11/portal events
   - Java Access Bridge events
   - terminal owned PTY buffer/input events
   - remote viewport screenshot/input events
3. Remove `FakeRecordingBackend::ready` from product code.
4. Block recording when only synthetic events would be available.
5. Store raw event evidence with timestamps and redaction.
6. Add heartbeat that reflects real event source liveness.
7. Update recorder UI to show active capture source and latest real event.

## Acceptance Tests

1. Starting each supported recording target either captures a real event stream or blocks.
2. No production backend writes `"fake backend heartbeat"`.
3. Recorder test matrix includes at least one real event per backend fixture.
4. Normalisation fails if only synthetic/session-started events exist.
5. UI shows capture state transitions accurately.

