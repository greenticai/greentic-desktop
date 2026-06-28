# PR-83 - Recording Capture Must Produce Executable Workflows

## Goal

Make recording produce typed `DesktopWorkflow` / `RunnerDefinition` manifests from actual captured events, annotations, and observations, with honest blocked states when capture is unavailable.

## User Outcome

When a user records opening a desktop app and creating data, Greentic captures enough information to replay the workflow. If the OS/app cannot be observed or controlled, Greentic says exactly what is missing rather than creating an empty or fake runner.

## Current Evidence

- `FakeRecordingBackend` can emit a fake heartbeat event.
- macOS recording backend currently appends a single focused-app event, not the user’s actions.
- Recording normalization compiles from raw events, but raw desktop events are not real user input streams.
- GUI recording test path uses `runner_test_outputs_from_yaml`, which fabricates outputs.

## Problem

A user can try to record Excel/LibreOffice/any desktop app, but the recording does not capture reliable app open, UI input, command, save, or output events. Recording cannot be trusted until raw events map to replayable semantic actions.

## Scope

1. Remove fake recording backend from production runtime.
2. Add capture-state rules:
   - `recording` only if real event source is active
   - `observe_only` if screenshots/UI tree are available but input events are not
   - `blocked` if required permissions/sources are missing
3. Implement real event ingestion contracts per adapter:
   - web: Playwright/CDP event stream and DOM snapshots
   - macOS: AX focused app/window/tree snapshots plus input-event source
   - Windows: UIA event stream plus input source
   - Linux: X11/AT-SPI event stream where available
   - Java: Java Accessibility events
   - terminal: PTY/input/output buffer stream
4. Normalize raw events into `DesktopWorkflow` actions and output extractors.
5. Preserve user annotations for inputs, secrets, outputs, and assertions.
6. Finalize recordings as typed runner manifests.

## Acceptance Tests

1. Starting desktop recording without a real event source returns blocked and cannot be finalized as a runnable runner.
2. A web fixture recording captures navigate, fill, click, observe/extract, then replays successfully.
3. A terminal fixture recording captures provided inputs and output buffer extraction, then replays successfully.
4. Native desktop recording tests use a controlled fixture app, not Excel or calculator-specific code.
5. Empty recordings cannot be normalized into passing runners.

