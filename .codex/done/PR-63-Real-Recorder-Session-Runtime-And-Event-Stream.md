# PR-63 - Real Recorder Session Runtime and Event Stream

## Goal

Replace the current file-only recording lifecycle with a running recorder runtime that starts adapter capture, streams structured events into the recording session, writes evidence, and reports honest capture status to the GUI and CLI.

## Problem

The current recorder creates `manifest.yaml`, `raw/events.jsonl`, and lifecycle events, but it never starts a native recorder or polls an adapter for new events.

Concrete gaps:

- `start_recording_session` writes `session_started` and returns `Recording`.
- `pause`, `resume`, `stop`, and `cancel` only update manifest state.
- `DesktopAdapter::record_event()` exists, but no session runtime owns adapters or drains events.
- GUI raw event counts only reflect lifecycle and marker events.
- `normalise_recording` cannot produce useful steps because no real click/type/read/window events are captured.

## User Outcome

When a user clicks "Start recording", Greentic starts a capture worker for the selected target. The GUI shows whether native capture is active, what adapter is capturing, how many real events and screenshots were captured, and why capture is blocked if permissions or dependencies are missing.

## Design

Add a recorder runtime crate/module around the existing session model:

```rust
pub trait RecordingBackend: Send + Sync {
    fn backend_id(&self) -> &'static str;
    fn capabilities(&self) -> Vec<String>;
    fn probe(&self) -> RecordingProbe;
    fn start(&self, ctx: RecordingContext) -> Result<Box<dyn RecordingHandle>, RecordingError>;
}

pub trait RecordingHandle: Send {
    fn pause(&mut self) -> Result<(), RecordingError>;
    fn resume(&mut self) -> Result<(), RecordingError>;
    fn poll(&mut self) -> Result<Vec<RawRecordingEvent>, RecordingError>;
    fn stop(&mut self) -> Result<RecordingStopSummary, RecordingError>;
}
```

The runtime owns:

- session manifest state
- selected backend handle
- event append loop
- evidence store writer
- redaction processor
- heartbeat/status file
- crash recovery lock

## Event Model

Use a typed `RawRecordingEvent` with serde instead of ad-hoc JSON strings:

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RawRecordingEvent {
    SessionStarted(SessionEvent),
    AppActivated(AppEvent),
    WindowFocused(WindowEvent),
    Click(InputEvent),
    KeySequence(InputEvent),
    TextCommitted(TextInputEvent),
    ElementObserved(ElementObservationEvent),
    ScreenshotCaptured(ScreenshotEvent),
    ClipboardChanged(ClipboardEvent),
    TerminalInput(TerminalInputEvent),
    TerminalOutput(TerminalOutputEvent),
    BrowserNavigation(BrowserNavigationEvent),
    Marker(UserMarkerEvent),
    Error(RecordingErrorEvent),
}
```

Each event should include:

- monotonic sequence number
- wall-clock timestamp
- backend ID
- app/window/process metadata where available
- stable locator candidates
- fallback coordinates only as a last resort
- optional `evidence_ref`
- redaction status

## Backend Plan

1. Add `RecordingRuntime` in `crates/greentic-desktop-recorder`.
2. Replace string rendering in `append_raw_event` with serde JSONL.
3. Add `recording_status.json` next to the manifest:
   - `capture_state`: `inactive | starting | active | paused | blocked | failed | stopped`
   - `backend_id`
   - `event_count`
   - `screenshot_count`
   - `last_event_at`
   - `blocked_reasons`
4. Add backend registry:
   - browser -> web recorder
   - desktop -> platform-native recorder, then vision fallback
   - remote -> screen/input recorder with explicit safety limits
   - terminal -> terminal recorder
5. Add explicit runtime APIs:
   - `start_recording_capture(session_id)`
   - `poll_recording_capture(session_id)`
   - `stop_recording_capture(session_id)`
6. Persist enough process state so a crashed GUI can recover a session as `failed` with evidence intact.

## GUI/API Changes

Extend `RecordingSummaryDto` with:

- `captureState`
- `captureBackend`
- `captureBlockedReasons`
- `realEvents`
- `screenshots`
- `lastEventAt`

The GUI must stop showing a generic timer when capture is inactive. It should show "Capture blocked" with exact remediation from the backend probe.

## Reuse Existing Libraries

Use existing crates where they reduce risk:

- `serde` / `serde_json` for event JSONL.
- `notify` or polling-free internal channels only if needed for status updates.
- `tracing` for structured recorder logs.
- `time` or `chrono` only if the workspace already accepts it; otherwise keep current timestamp helpers and add monotonic sequence numbers.

Do not invent a custom async runtime unless the repo already adopts one. A single managed worker thread per recording session is enough for the first real implementation.

## Acceptance Criteria

- Starting a recording starts a real backend handle or returns a blocked status with actionable reasons.
- `raw/events.jsonl` contains typed real events, not only lifecycle and markers.
- Pause/resume affects backend event capture.
- Stop drains pending events and writes a final summary.
- GUI event counts distinguish lifecycle/marker events from captured app events.
- Tests cover lifecycle, blocked backend, active backend, pause/resume drain behavior, and crash recovery.

## Test Plan

- Unit test recorder runtime with an in-memory fake backend.
- Regression test current GUI API lifecycle against typed JSONL.
- Add a fake backend that emits click, text, screenshot, and output events; verify normalization receives them.
- Manual smoke test: start recording, perform any app action, verify event count increments.

## Risks

- Long-running background workers need cleanup when the GUI process exits.
- Event streams can contain secrets; redaction must happen before writing raw JSONL.
- Backends can produce duplicate events; PR-66 handles normalization and de-duplication.

