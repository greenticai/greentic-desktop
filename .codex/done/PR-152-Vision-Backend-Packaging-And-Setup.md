# PR-152 - Vision Backend Packaging and Setup

## Goal

Replace the `GREENTIC_VISION_BACKEND_COMMAND` dead-end with an installable/configurable vision backend for screenshots, OCR, region matching, and input fallback.

## User Outcome

When structured adapters cannot inspect an app, users can enable vision fallback from Settings and get real screenshots, OCR text extraction, annotated evidence, and region clicks without manually inventing a backend command.

## Current Evidence

- `VisionAdapter` already expects an external backend command.
- Health reports `sidecar_missing` with only `Set GREENTIC_VISION_BACKEND_COMMAND`.
- There is no GUI setup action or packaged local backend.
- Vision fallback is central for remote desktop and opaque desktop apps, but currently not usable out of the box.

## Scope

1. Package a local vision backend sidecar using existing crates/tools where practical:
   - `xcap` or platform capture APIs for screenshots.
   - Tesseract/`leptess` or a platform OCR provider when available.
   - `image`/`imageproc` for template matching and annotation.
   - platform input backend for click region execution.
2. Add setup detection:
   - screen recording permission
   - OCR engine availability
   - input permission
   - backend command installed
3. Persist backend command/config in Greentic runtime config.
4. Provide setup/fix buttons:
   - install local backend
   - open OS permissions
   - test screenshot
   - test OCR
5. Add evidence output:
   - screenshot before/after
   - OCR text
   - match boxes and confidence
   - clicked coordinates
6. Add confidence thresholds and fail closed below threshold.
7. Ensure vision fallback is selected only when structured adapters cannot satisfy the requested capability.

## Acceptance Tests

1. Fresh install reports exactly which vision sub-capability is missing.
2. Setup configures a backend command or explains the platform-specific dependency that must be installed.
3. Fixture screenshot OCR extracts expected text.
4. Region click fixture produces before/after screenshots and passes only when the target changed.
5. Low-confidence matches fail with evidence and never produce fake success.
