# PR-63 - Real Recording Capture Runtime

## Goal

Replace the current session-only recorder with a real capture runtime that can ingest events from web, native desktop, Java, terminal, and remote/vision recorders while a user performs a task.

## Problem

The GUI can create a recording session and marker files, but no capture backend is connected. A user can click "Record", switch tabs or apps, perform work, and the raw event stream stays empty. This is worse than incomplete because it looks like recording is active when it is only a lifecycle placeholder.

## Scope

1. Add a `RecordingBackend` trait in `greentic-desktop-recorder`.
2. Add a `RecordingBackendRegistry` owned by the runtime/GUI state.
3. Add a long-running capture worker per active recording session.
4. Add explicit capture state:
   - `starting`
   - `recording`
   - `paused`
   - `blocked`
   - `stopped`
   - `failed`
5. Write raw captured events continuously to `events.jsonl`.
6. Add heartbeat/status metadata so the GUI can show whether capture is actually active.
7. Make session start fail or return `blocked` when no backend can record the selected target.
8. Remove any UI/backend wording that implies recording is active when no capture backend is attached.

## Backend Contract

Add a backend trait similar to:

```rust
pub trait RecordingBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn target_kind(&self) -> RecordingTargetKind;
    fn preflight(&self, request: &RecordingStartRequest) -> RecordingPreflight;
    fn start(&self, request: RecordingStartRequest, sink: RecordingEventSink) -> RecordingHandle;
}
```

`RecordingEventSink` must support:

- append raw event
- append screenshot/evidence reference
- append observation snapshot
- append backend warning
- update heartbeat
- stop on cancellation

## Raw Event Envelope

Every backend must emit the same envelope:

```json
{
  "schema_version": "recording.event.v1",
  "session_id": "rec_...",
  "backend": "greentic.recording.web.playwright",
  "target_kind": "web",
  "timestamp": "2026-06-27T10:00:00Z",
  "sequence": 42,
  "event": {
    "kind": "click|type|navigate|observe|read|key|terminal_screen|screenshot",
    "target": {},
    "value": null,
    "redaction": "none|input_candidate|secret_candidate|redacted"
  },
  "evidence": {
    "screenshot_ref": null,
    "dom_snapshot_ref": null,
    "ui_tree_ref": null,
    "terminal_buffer_ref": null
  }
}
```

## GUI API Changes

Extend recording DTOs with:

- `captureState`
- `captureBackend`
- `captureHeartbeatAt`
- `captureBlockedReasons`
- `rawEvents`
- `observations`
- `screenshots`
- `lastEventSummary`

The GUI must show a red/yellow blocked state when:

- no backend is installed
- required OS permission is missing
- user picked a target that cannot be observed
- browser/terminal/native app was not launched or attached by Greentic

## Acceptance Criteria

- Starting a recording without an available backend does not pretend to record.
- Starting a recording with a fake test backend writes at least one event to `events.jsonl`.
- Pause/resume controls stop and resume event ingestion.
- Stop flushes and joins the worker cleanly.
- Cancel terminates the worker and marks the session cancelled.
- `GET /api/v1/recordings/{id}` reports capture state and raw event count from disk.
- No recording target shows "Recording" unless a backend heartbeat is active or a clear blocked reason is shown.

## Test Plan

- Unit tests for backend registry selection.
- Unit tests for event sink writing JSONL.
- Integration test with fake backend emitting click/type/observe events.
- API test for blocked session when backend preflight fails.
- API test for pause/resume/stop worker state.
- GUI mocked test that blocked capture is visibly distinct from active recording.

## Non-Goals

- This PR does not implement each real backend. It creates the required runtime seam so backend PRs can plug in.
- This PR does not normalize backend-specific events into final runner steps; that is PR-70.

