# PR-122 - Recording Capture Reliability and Replay Determinism

## Goal

Make recording produce deterministic primitive workflows that can be replayed against real fixture apps.

## User Outcome

When a user records a web or desktop action sequence, Greentic captures real events, derives stable inputs/outputs, and can replay the result without manual YAML editing.

## Current Evidence

- macOS recording uses runtime Swift CGEvent tap source injection.
- Windows/Linux recording reliability is not equivalent.
- Normalization from raw events to primitives is heuristic.
- Current tests validate event structs more than replay determinism.

## Scope

1. Stabilize capture backends:
   - web: Playwright/CDP capture.
   - macOS: packaged event tap/AX observer sidecar.
   - Windows: UIA event source.
   - Linux: AT-SPI/X11/Wayland-specific capture capabilities with honest limitations.
2. Define raw event schema v2:
   - target identity.
   - locator evidence.
   - timing.
   - input/output markers.
   - screenshots or UI tree refs.
3. Deterministic normalization:
   - raw events -> primitives.
   - primitives -> compiled steps.
   - stable ids.
   - no empty locators.
4. Recorder test UX:
   - show captured events.
   - show derived primitives.
   - show replay dry-run diagnostics.
5. Fixture replay tests:
   - record a web form.
   - replay it.
   - assert same output.

## File Targets

- `crates/greentic-desktop-recorder/src/lib.rs`
- `crates/greentic-desktop-web/src/lib.rs`
- `crates/greentic-desktop-macos/src/lib.rs`
- `crates/greentic-desktop-windows/src/lib.rs`
- `crates/greentic-desktop-linux/src/lib.rs`
- `crates/greentic-desktop-gui/src/lib.rs`

## Out of Scope

- Human-in-the-loop ML locator inference.
- Remote desktop recording beyond capability-gated basics.

## Acceptance Tests

1. Web fixture recording captures real field fill and click events.
2. Normalized primitives contain no empty locator fields.
3. Replayed runner returns the same fixture output.
4. Missing capture permissions block recording with a targeted message.
5. Recorder UI test displays captured event and replay diagnostic.

## Done Means

At least one recording path proves record-once/replay works against a real fixture.
