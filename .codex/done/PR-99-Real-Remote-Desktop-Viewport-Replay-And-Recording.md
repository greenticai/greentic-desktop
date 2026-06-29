# PR-99 - Real Remote Desktop Viewport Replay and Recording

## Goal

Implement real remote desktop automation for Greentic-owned RDP/VNC/WorkSpaces/browser-canvas sessions.

## User Outcome

Greentic can record and replay automation inside a controlled remote viewport with calibrated coordinates, screenshots, OCR, and keyboard/mouse input.

## Current Evidence

- `RemoteVisionRecordingBackend::start` emits a synthetic focused viewport event.
- Remote recording requires a Greentic-owned session but no real session runtime exists.

## Scope

1. Define remote viewport provider contract:
   - screenshot frame stream
   - input injection
   - coordinate calibration
   - focus/resize detection
   - session lifecycle
2. Implement at least one real provider:
   - browser canvas VNC/noVNC fixture, or
   - RDP/VNC controlled process, or
   - AWS WorkSpaces integration if credentials/environment are available.
3. Integrate with vision backend for OCR/click/read.
4. Record viewport screenshots and input events.
5. Persist calibration and evidence.
6. Fail when session is not Greentic-owned or calibration is missing.

## E2E Fixtures

1. Local noVNC/browser-canvas fixture with simple form app.
2. Optional AWS WorkSpaces smoke behind credentials.

## Acceptance Tests

1. Remote runner fills a real field and extracts real output in controlled viewport.
2. Missing calibration blocks before recording/replay.
3. Coordinate replay adjusts for viewport scale changes or fails with calibration error.
4. Recording produces screenshot-backed events.
5. Evidence includes before/after viewport screenshots.

