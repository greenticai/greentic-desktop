# PR-74 - LLM Planning UX, Traceability, and Controls

## Goal

Make the prompt creation and update experience transparent, controllable, and debuggable for users and developers.

## Problem

The GUI currently shows generated fields, steps, and YAML, but not how the LLM reached the result, which questions are blocking, which assumptions were made, which schema/repair attempts occurred, or why a provider failed. Settings can configure a provider, but planning does not expose enough diagnostics when reliability work is added.

## User Outcome

Users see:

- which LLM provider/model was used
- whether strict schema mode was available
- what questions need answers
- what assumptions were accepted
- why a draft/update was repaired or blocked
- what changed before applying updates
- whether generated runner schemas match MCP inputs/outputs

## Create Wizard Updates

Add panels for:

- Requirements
- Questions
- Assumptions
- Planner route
- Inputs/secrets/outputs
- Workflow preview
- Validation warnings
- Planning trace

The first step should not jump directly from prompt to draft when blocking questions exist.

## Update Wizard Updates

For an existing runner:

- Open a dedicated update wizard, not a small inline correction box.
- Show current runner summary and schemas.
- Ask for update prompt.
- Show questions if needed.
- Show semantic diff.
- Run test.
- Apply or discard.

## Diagnostics

Expose local API endpoint:

```http
GET /api/v1/planner/traces/{trace_id}
```

Return safe diagnostics:

- provider/model
- structured output mode
- schema version/hash
- attempts count
- validation summaries
- repair summaries
- final questions/blockers

Do not expose raw secret-bearing context. Raw prompts are acceptable only if they went through redaction.

## Controls

Add advanced settings:

- max repair attempts
- prefer local heuristic / configured provider
- allow live provider calls
- require review before save/apply
- store planning traces
- redact prompt traces

Defaults should favor safety:

- repair attempts enabled
- review before save/apply enabled
- traces enabled with redaction
- live provider only when configured

## Acceptance Criteria

- Blocking questions are displayed before draft creation.
- Non-blocking assumptions are displayed and can be accepted or edited.
- Update flow has a semantic diff and explicit apply.
- Planning trace is inspectable from GUI and API.
- Provider/schema failures show actionable red error messages.
- Settings can tune repair attempts and trace retention.

## Test Plan

- Frontend mocked tests for create with questions, create with assumptions, and create success.
- Frontend mocked tests for update questions, diff, test, apply.
- Backend API tests for trace redaction.
- Provider failure test shows user-friendly error.
- Snapshot tests for semantic diff rendering.

## Risks

- Too much detail can overwhelm users. Keep trace details behind an expandable diagnostics area.
- Trace storage can contain sensitive business prompts. Redaction and retention controls are mandatory.

