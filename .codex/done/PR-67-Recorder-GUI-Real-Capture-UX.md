# PR-67 - Recorder GUI for Real Capture, Evidence Review, and Repair

## Goal

Update the recording wizard so it controls and explains the real recorder runtime instead of looking like a working recorder while only creating session files.

## Problem

The current GUI has a recording screen, timer-like display, marker buttons, normalise, test, and save. It is useful scaffolding, but it can mislead users because no native event stream is active.

## User Outcome

The user sees exactly what is happening:

- capture backend selected
- capture active/blocked/paused state
- required permissions or missing dependencies
- real event count
- screenshots/evidence count
- latest captured action
- warnings when events are coordinate-only or weakly locatable

## UX Changes

### Target Selection

`GET /api/v1/recording-targets` should return dynamic target availability:

- target ID
- label
- recommended backend
- supported on this OS/session
- permission requirements
- setup actions
- known limitations

Disable targets that cannot capture on the current machine, unless a fallback backend is available.

### Recording Screen

Replace fake timer behavior with live status:

- `Capture active` when backend is emitting events.
- `Capture blocked` when permissions/dependencies are missing.
- `Waiting for app activity` when active but no app events observed yet.
- `Paused` when paused.
- `Stopped` after draining.

Show recent events:

- app/window
- action
- target summary
- redaction marker
- evidence thumbnail if available

### Permission Repair

When setup opens system settings, show exact app/process name that needs permission:

- macOS: the signed app/binary currently running Greentic Desktop.
- Windows: whether UI Automation is available and whether elevated target apps require Greentic to run elevated.
- Linux: session type, AT-SPI status, xdg portal status, X11/Wayland limitations.

### Markers

Marker buttons should operate on the selected recent event or screenshot region, not a generic string:

- Mark selected value as input.
- Mark selected value as secret.
- Mark selected observed value as output.
- Mark selected step as assertion.
- Mark selected region as visual locator.

### Review

Review should show:

- normalized semantic steps
- associated raw event/evidence links
- inputs/secrets/outputs/extractors
- warnings and open questions
- editable locators and names

### Test and Save

The final page should run the generated runner through the same replay engine used by the runner page. It should not return sample output.

## API Changes

Add or extend:

```http
GET /api/v1/recordings/{session_id}
GET /api/v1/recordings/{session_id}/events
GET /api/v1/recordings/{session_id}/evidence
POST /api/v1/recordings/{session_id}/markers
POST /api/v1/recordings/{session_id}/normalise
POST /api/v1/recordings/{session_id}/run
```

Keep polling first. Server-sent events can be added later after the JSON API is stable.

## Frontend Plan

1. Extend DTOs in `frontend/automate-hub/src/lib/types.ts`.
2. Add API calls in `frontend/automate-hub/src/lib/api.ts`.
3. Poll session status while `captureState` is `starting`, `active`, `paused`, or `blocked`.
4. Build a recent-events panel.
5. Replace static marker buttons with selected-event marker controls.
6. Replace `sample-output` test result with backend replay result.
7. Add clear empty states for blocked/no-events/no-output.

## Acceptance Criteria

- The recording screen never implies real capture is active when it is not.
- User can see live real event counts increment during supported recording.
- Blocked permission states show exact remediation and affected process/app.
- User markers attach to concrete events, values, or evidence regions.
- Review page can inspect normalized steps and open questions.
- Test page uses actual replay output/errors.

## Test Plan

- Frontend tests with mocked `active`, `blocked`, `paused`, and `no events` states.
- Backend GUI API tests for status/events/evidence endpoints.
- Playwright GUI smoke test with fake recorder backend.
- Manual desktop test on each OS once PR-64 lands.

## Risks

- Evidence thumbnails may contain sensitive data. Use local authenticated artifact URLs and redact metadata.
- The first real implementation may not support all target types; unsupported states must be explicit.

