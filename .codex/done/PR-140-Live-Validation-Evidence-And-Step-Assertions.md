# PR-140 - Live Validation Evidence And Step Assertions

## Goal

Extend replay so every live step can assert expected side effects and write evidence that proves what happened.

## User Outcome

A passing run means more than “no Rust error”: it means the expected app/file/output state was actually observed.

## Current Evidence

- Runners have reported `Test passed` while files were missing or output values were placeholders.
- The Excel workflow updated the file at one point but still left dialogs open, and the result needed both checks.
- Evidence references exist, but they do not consistently include live observations.

## Scope

1. Add `LiveStepAssertion` to the runner schema:
   - `frontmost_app_is`.
   - `window_exists`.
   - `no_blocking_modal`.
   - `file_exists`.
   - `file_changed_since_step_start`.
   - `output_path_exists`.
   - `text_visible`.
   - `element_focused`.
2. Let validation runs inject assertions even if the runner YAML does not contain them.
3. Capture before/after step snapshots:
   - file state.
   - app/window state.
   - modal state.
   - screenshot on failure.
4. Fail if an output extractor returns a path that does not exist.
5. Fail if a `save_as` step updates a file but leaves a modal open.
6. Add a compact human-readable failure format:
   - failed step id.
   - action/capability.
   - expected assertion.
   - observed state.
   - modal text/buttons if present.
7. Preserve full JSON details in the evidence bundle.

## File Targets

- `crates/greentic-desktop-runner-schema/src/lib.rs`
- `crates/greentic-desktop-replay/src/lib.rs`
- `crates/greentic-desktop-runtime/src/lib.rs`
- `crates/greentic-desktop-gui/src/lib.rs`
- `examples/runners/*.yaml`

## Out of Scope

- Adding LLM planning changes.
- Changing non-live CI tests into live tests.

## Acceptance Tests

1. A runner that outputs `/tmp/missing.docx` fails validation with `output_path_missing`.
2. A save step that changes a file but leaves a modal open fails with `blocking_modal_remaining`.
3. The failure message includes the step id, modal text, and button labels.
4. Evidence JSON includes before/after snapshots per step.
5. Non-live replay remains available and does not require OS probes.

## Done Means

Replay has a live proof layer that catches fake success and explains real desktop failures at the step where they occur.
