# PR-68 - Remote Desktop Recording Through Vision and Input

## Goal

Make remote desktop recording real for environments where semantic APIs are unavailable by recording screenshots, OCR/vision observations, and user input in a constrained Greentic-owned remote session.

## Problem

Remote desktops usually do not expose app UI trees to the local machine. The current remote recording choice can create a session but does not capture screen transitions, clicks, typed text, or output regions.

## Scope

1. Add a remote/vision recording backend plugged into PR-63.
2. Support Greentic-owned remote sessions first:
   - RDP/VNC/WorkSpaces client window launched or selected by Greentic
   - browser-based remote desktop canvas where Playwright can observe the canvas
3. Capture:
   - screenshot before/after input
   - click coordinates relative to remote viewport
   - typed text and key sequences
   - OCR text snapshots
   - visual locator regions
   - user-marked input/output/assertion regions
4. Normalize to:
   - `remote.focus_session`
   - `remote.click_region`
   - `remote.type_text`
   - `remote.press_key`
   - `remote.wait_for_text`
   - `remote.extract_text_region`
   - `remote.assert_text`
5. Add calibration for remote viewport origin, scale, and DPI.
6. Require explicit approval for screen capture and keyboard/mouse control.

## UX Requirements

Remote recording must be honest:

- If screen capture is missing, block before recording.
- If keyboard/mouse control is missing, allow observe-only mode or block replayable recording.
- If Greentic cannot identify the remote viewport, ask the user to select/calibrate it.

## Acceptance Criteria

- Recording a browser-based remote desktop fixture captures clicks, text entry, screenshots, and OCR output.
- Normalized runner can replay against the fixture and extract a result.
- Missing screen capture permission blocks recording.
- Missing input permission blocks replayable recording or explicitly marks observe-only.
- Visual locator regions are included in evidence.

## Test Plan

- Playwright canvas fixture for remote desktop simulation.
- Screenshot/OCR unit tests with deterministic fixture images.
- Coordinate calibration tests.
- Normalization test from remote JSONL to runner YAML.
- Replay test against simulated remote canvas fixture.

## Done Means

"Remote desktop task" records real visual/input events from a controlled remote viewport or blocks with a concrete reason.

