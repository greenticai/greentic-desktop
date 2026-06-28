# PR-78 - Stop Fake Success and Hard-Coded Demo Outputs

## Goal

Make runner `run`, runner `test`, planner draft `test`, recording `test`, and MCP tool calls execute the real replay pipeline or fail with a concrete missing-capability error. Remove product runtime code that fabricates calculator results, `sample-output`, or generic passed steps.

## User Outcome

When a user creates or records a runner, the UI only says it works if Greentic actually opened/attached to the target, provided the declared inputs, executed the steps, extracted the declared outputs, and saved evidence. A spreadsheet-style prompt can fail honestly with “missing app/file/create-row capability” instead of showing a fake green result.

## Current Evidence

- `crates/greentic-desktop-gui/src/lib.rs` has `runner_outputs_json`, `runner_test_outputs_from_yaml`, and `calculator_result`.
- `runner_action_json` returns passed for `validate`, `test`, and `run` before dispatching any real adapter.
- `planner_draft_action_json(.../test)` and `test_runner_edit_draft_json` return `"status":"passed"` and fabricated output values.
- MCP `tools/call` builds a passed response from `runner_outputs_json`.

## Problem

The GUI can look complete while the product is not executing desktop automation. This allowed calculator-specific logic to hide missing generic replay, output extraction, and adapter dispatch. It also means prompts like “create/open a spreadsheet, add a row, save” cannot work because the system never proves file/app automation capabilities exist.

## Scope

1. Delete calculator-specific runtime result handling from GUI/MCP paths.
2. Delete `{field}-sample-output` and `"sample-output"` from product test/run paths.
3. Introduce one GUI-owned `execute_runner_manifest` service that:
   - parses the saved runner manifest
   - builds a `ReplayRequest`
   - resolves input and secret values
   - selects installed adapters
   - calls `greentic-desktop-replay`
   - persists evidence
   - returns real outputs or structured errors
4. Use the same service for:
   - planner draft test
   - recording test
   - runner run
   - runner test
   - MCP tool call
   - runner edit draft test
5. Keep test fixtures allowed in tests, but isolate them behind test-only adapters or fixture binaries.

## Non-Goals

- Do not add spreadsheet-specific behavior.
- Do not add Excel-specific parsing.
- Do not special-case calculator.

## Implementation Notes

- Product code may include fixture adapters only under `#[cfg(test)]` or an explicit fixture feature.
- `validate` can remain schema/capability validation, but must not claim execution.
- `test` should be an execution dry run with user-provided inputs against either the real target or an explicit fixture target.
- `run` should execute the real saved runner.
- Return `runner.execution_missing_capability`, `runner.output_extraction_failed`, or `runner.adapter_unavailable` instead of fake outputs.

## Acceptance Tests

1. A runner declaring output `result` with no extractor and no adapter observation fails with `runner.output_extraction_failed`.
2. A runner declaring calculator-like inputs no longer returns `2` unless an adapter/test fixture actually emits that output.
3. Planner draft test for a generic desktop workflow calls the replay service.
4. MCP tool call uses the same replay service as the GUI run button.
5. Repository scan test fails if product runtime code contains `sample-output`, `calculator_result`, or hard-coded `{field}-sample-output`.

## Regression Scenario

Prompt:

> Ask for the name of a spreadsheet. In /tmp create the spreadsheet if it does not exist already. Otherwise open it. Add a new line to the spreadsheet with the name and email that the user provided. Save the changes.

Expected for this PR:

- If required file/app/row-entry capabilities are not yet implemented, the runner test fails with explicit missing capabilities.
- It must not pass with fabricated output.

