# PR-40 - Recording GUI Wizard and Recorder Bridge

## Goal

Connect the `/create` recording wizard to the real recording lifecycle so a user can name a runner, choose a recording target, start/pause/stop/cancel recording, mark inputs/secrets/outputs/assertions, normalise the recording, test it, and save a runner.

## User Outcome

A user can click "Record the task", perform the workflow once, and Greentic Desktop turns it into a reusable runner without using the terminal.

## Current State

- Recording wizard is local React state only.
- Backend has recording lifecycle functions:
  - `start_recording_session`
  - `pause_recording_session`
  - `resume_recording_session`
  - `stop_recording_session`
  - `cancel_recording_session`
  - `normalise_recording`
  - `finalise_recording`
  - marker/note commands through CLI handlers
- The current implementation models recording sessions and raw event files, but GUI needs status polling and action endpoints.

## Scope

1. Add recording API endpoints.
2. Map recording modes in the UI to adapters/profiles.
3. Add live recording status polling.
4. Add marker actions for input/output/secret/assertion/note.
5. Add normalise/finalise/test/save flow.
6. Add evidence/screenshot references to review screens.

## API Design

```http
POST /api/v1/recordings
GET /api/v1/recordings
GET /api/v1/recordings/{session_id}
POST /api/v1/recordings/{session_id}/pause
POST /api/v1/recordings/{session_id}/resume
POST /api/v1/recordings/{session_id}/stop
POST /api/v1/recordings/{session_id}/cancel
POST /api/v1/recordings/{session_id}/mark-input
POST /api/v1/recordings/{session_id}/mark-output
POST /api/v1/recordings/{session_id}/mark-secret
POST /api/v1/recordings/{session_id}/add-assertion
POST /api/v1/recordings/{session_id}/note
POST /api/v1/recordings/{session_id}/normalise
POST /api/v1/recordings/{session_id}/finalise
POST /api/v1/recordings/{session_id}/test
```

## Recording Target Mapping

The UI choices should map to backend adapter/profile values:

- Browser task -> Playwright/web adapter profile
- Desktop app task -> platform-native adapter if available, fallback to vision/input
- Remote desktop task -> vision plus keyboard/mouse constrained profile
- Terminal/mainframe task -> terminal/TN3270 profile

The API should return disabled choices when required adapters are missing.

## Backend Plan

### Recording DTOs

Expose:

- session ID
- name
- state
- elapsed duration
- selected profile/adapter
- active app/window if known
- raw events count
- marked inputs/secrets/outputs/assertions
- draft runner path
- normalized step summaries
- screenshot/evidence references

### Live Status

Use polling first:

```http
GET /api/v1/recordings/{session_id}
```

Add server-sent events or WebSocket only after the polling API is stable.

### Normalise and Review

Normalise should return:

- extracted steps
- inferred inputs/outputs
- redaction summary
- warnings where locators are weak
- YAML preview

### Finalise

Finalise should write the runner into the configured runner storage path and return its runner ID/path.

## Frontend Plan

- Step 1 posts runner name metadata only after user confirms.
- Step 2 fetches available recording targets and starts session.
- Step 3 polls session status and wires pause/stop/cancel buttons.
- Marker buttons call recording marker endpoints.
- Step 4 displays normalized real steps and warnings.
- Step 5 tests and saves/finalises the recording.

## Acceptance Criteria

- Recording wizard starts a real recording session and shows real session ID/state.
- Pause/resume/stop/cancel buttons change backend state.
- Marker buttons persist into recording metadata.
- Normalise produces a draft runner from recorded events.
- Finalise writes a runner discoverable by `/runners` and CLI `runner list`.
- UI handles invalid state transitions without crashing.

## Test Plan

- Backend API tests for every lifecycle endpoint.
- Invalid-state tests, for example stopping an already cancelled session.
- Frontend smoke test with mocked API for record flow.
- Manual test on Windows, macOS, and Linux with platform-specific permissions.

## Risks

- Real OS event capture may still be modeled rather than native in early iterations. The UI must expose honest state: "recording session active" versus "native event capture active".
- Long-running recording requires robust cleanup on process exit.

