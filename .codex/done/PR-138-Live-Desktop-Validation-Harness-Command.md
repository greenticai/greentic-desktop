# PR-138 - Live Desktop Validation Harness Command

## Goal

Add an explicit live validation harness that runs real desktop workflows against the current machine and proves side effects instead of relying on model-only tests or manual user observation.

## User Outcome

A developer can run one command and know whether a runner actually opened apps, handled dialogs, produced files, extracted outputs, and left the desktop in a clean state.

## Current Evidence

- The Excel runner returned misleading failures and earlier false successes because tests checked runner status but not live GUI state.
- Save/replace dialogs can remain open after a workflow while the CLI output is incomplete or misleading.
- Existing CI tests are useful, but they cannot prove native desktop automation on a real logged-in desktop session.

## Scope

1. Add `greentic-desktop desktop validate` as a first-class CLI command.
2. Support validating:
   - a runner YAML path.
   - an installed runner id.
   - input values from `--input`, `--inputs-json`, and `--inputs-file`.
3. Add live validation options:
   - `--expect-file PATH`
   - `--expect-file-changed PATH`
   - `--expect-output KEY=VALUE`
   - `--expect-no-modal`
   - `--expect-frontmost-app APP`
   - `--screenshots always|on-failure|never`
   - `--timeout-ms N`
4. Return structured JSON by default when `--json` is passed:
   - status.
   - runner id.
   - step diagnostics.
   - live assertions.
   - evidence bundle path.
   - blocking dialog details.
5. Fail closed:
   - if a workflow step times out.
   - if any expected file is missing or unchanged.
   - if a blocking modal remains.
   - if an output points to a non-existent file when it claims a saved path.
6. Keep the existing `--run` command, but recommend `desktop validate` for live desktop qualification.

## File Targets

- `crates/greentic-desktop-cli/src/lib.rs`
- `crates/greentic-desktop-runtime/src/lib.rs`
- `crates/greentic-desktop-replay/src/lib.rs`
- `crates/greentic-desktop-gui/src/lib.rs`
- `docs/live-validation.md`

## Out of Scope

- Running live validation in normal CI by default.
- Making any specific app workflow pass.
- Replacing adapter implementations.

## Acceptance Tests

1. `cargo run --bin greentic-desktop -- desktop validate --workflow examples/runners/macos-excel-tabs-formula-save.yaml --input workbook_path=/tmp/test.xls --input source_number=10 --expect-file-changed /tmp/test.xls --expect-no-modal` returns non-zero when the file is not updated.
2. The same command returns non-zero when an app modal remains open, with the modal text and button labels in the error.
3. `--json` output includes `liveAssertions`, `evidenceBundle`, and per-step diagnostics.
4. `--screenshots on-failure` writes a screenshot path into the evidence bundle on failure.
5. Existing `--run`, `--import`, and `--export` behavior is unchanged.

## Done Means

There is one explicit local command that proves a runner worked on the live desktop or explains exactly why it did not.
