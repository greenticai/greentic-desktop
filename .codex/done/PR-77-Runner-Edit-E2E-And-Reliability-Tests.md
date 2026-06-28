# PR-77 - Runner Edit E2E and Reliability Tests

## Goal

Add automated coverage proving that existing runners can be extended through LLM prompting and then run through GUI and MCP without regressions.

## User Outcome

The edit flow is trusted because tests cover the real path: click edit, prompt a change, review/answer questions, test, apply, run, and call through MCP.

## Problem

Previous UI paths regressed because tests only covered narrow API calls or placeholder flows. Editing existing runners needs end-to-end tests that exercise the actual user flow and backend contracts.

## Scope

1. Add GUI end-to-end edit tests.
2. Add backend edit-draft and apply integration tests.
3. Add LLM mocked-provider tests for patch planning.
4. Add MCP post-edit behavior tests.
5. Add negative tests for invalid JSON, missing API keys, open questions, conflicts, and failing replay.

## Test Matrix

### Calculator Runner

Base runner:

- inputs: `number_1`, `number_2`, `operation`
- output: `result`

Edit prompts:

- “Also allow operation aliases plus, minus, divide, multiply.”
- “Add a precision input and round the result.”
- “Return both the result and the displayed expression.”

Expected:

- new inputs/outputs are inferred from the edit instruction
- existing inputs remain unless explicitly removed
- test uses dynamic fields
- apply updates the same runner
- MCP call returns the new outputs

### Web Runner

Base runner opens a web calculator or test app.

Edit prompt:

- “Also read the history line under the result.”

Expected:

- web output extractor is added
- test uses Playwright/web adapter
- no desktop-only assumptions are introduced

### Desktop Runner

Base runner launches a native calculator or generic app fixture.

Edit prompt:

- “After calculating, copy the result to clipboard and return it.”

Expected:

- native/clipboard capability is selected only if available
- missing permission is surfaced as a blocking requirement

### Terminal Runner

Base runner executes a CLI command.

Edit prompt:

- “Also return stderr when the command fails.”

Expected:

- terminal output extractor is added
- failure behavior is explicit

## Playwright Coverage

Use Playwright against the GUI:

1. Open `My Runners`.
2. Click `Edit`.
3. Verify the selected runner is loaded.
4. Enter edit prompt.
5. Generate changes.
6. Answer any open questions.
7. Verify diff shows added/changed fields.
8. Enter sample values.
9. Test proposed runner.
10. Apply.
11. Verify runner page shows updated runner.
12. Run runner from UI.
13. Verify MCP tool list/call uses same updated runner.

## Backend Coverage

- `edit_draft_create_loads_existing_runner`
- `edit_patch_planner_uses_configured_llm_provider`
- `edit_patch_planner_repairs_invalid_json`
- `edit_patch_planner_returns_open_questions`
- `edit_apply_requires_matching_source_checksum`
- `edit_apply_requires_schema_validity`
- `edit_apply_refreshes_mcp_tool`
- `edit_apply_creates_version_history`
- `edit_restore_reverts_runner_behavior`

## Reliability Requirements

- No hard-coded company names, emails, CRM assumptions, or sample Acme data.
- Tests must fail if generated input/output lists are empty for prompts that clearly require fields.
- Tests must fail if the edit flow creates a new runner instead of updating the source runner.
- Tests must fail if MCP requires a separate publish step after apply.
- Tests must fail if invalid LLM JSON is accepted without repair/diagnostics.

## Acceptance Criteria

- End-to-end tests cover existing-runner edit from UI through apply and MCP.
- Mock LLM tests prove strict schema and repair behavior.
- At least one test exercises open questions.
- At least one test exercises conflict detection.
- CI catches regressions where `Edit` routes to new-runner creation.
