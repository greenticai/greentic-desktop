# PR-98 - Real Vision Screenshot, OCR, and Input Backend

## Goal

Replace the seeded `VisionAdapter` with real screenshot capture, OCR/template matching, coordinate targeting, and input execution.

## User Outcome

When structured adapters cannot access an app, Greentic can use visible screen evidence to locate text/buttons, click regions, compare baselines, and extract visible output while producing audit evidence.

## Current Evidence

- `VisionAdapter::execute` uses in-memory screenshot/text matches.
- It returns `"vision step accepted"` without real screen capture, OCR, or input.

## Scope

1. Add screenshot provider abstraction:
   - macOS ScreenCaptureKit/CGWindow.
   - Windows Graphics Capture or BitBlt fallback.
   - Linux portal/X11 capture.
2. Add OCR provider:
   - Tesseract or platform OCR where available.
   - Pluggable cloud/local OCR provider only with explicit configuration.
3. Add visual matching:
   - text bounding boxes
   - image/template match
   - button/region detection
4. Add input executor integration with platform input APIs from native adapter PRs.
5. Implement:
   - `vision.screenshot`
   - `vision.find_text`
   - `vision.find_button`
   - `vision.click_region`
   - `vision.compare_baseline`
   - `vision.assert_visual`
   - `vision.extract_text`
6. Persist before/after screenshots and annotated regions.
7. Add confidence thresholds and fail when confidence is too low.
8. Avoid claiming vision is installed/healthy until screenshot + OCR + input dependencies are ready for the requested operation.

## E2E Fixtures

1. Static desktop screenshot fixture for deterministic unit tests.
2. Real visible fixture app for click/extract E2E.

## Acceptance Tests

1. Missing screen permission hides screenshot/click capabilities.
2. OCR extraction returns real visible text from a fixture screen.
3. Click region sends real platform input and produces after-screenshot evidence.
4. Low-confidence matches fail with evidence, not pass.
5. Vision fallback is only selected when structured adapters are unavailable or insufficient.

