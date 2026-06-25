# PR-39 - Prompt-to-Runner GUI Wizard

## Goal

Connect the `/create` prompt wizard to the real planner, schema validation, policy checks, runner draft rendering, test execution, and save flow.

## User Outcome

A non-technical user can describe a task, review the inferred inputs/outputs/steps, test the draft runner, and save it from the browser UI.

## Current State

- The prompt wizard is local React state only.
- Backend has:
  - `plan_prompt_with_default_llm`
  - `PlanningContext`
  - `PlannerOptions`
  - `RunnerDraft`
  - `save_draft_runner`
  - schema and policy diagnostics
- CLI command `runner plan` exists, but not a GUI API.

## Scope

1. Add prompt planning API endpoints.
2. Add persisted draft state so the wizard can move across steps.
3. Replace hard-coded inputs/outputs/steps/YAML with real planner output.
4. Add edit operations for inputs, outputs, and steps.
5. Add test and save endpoints.

## API Design

```http
POST /api/v1/planner/drafts
GET /api/v1/planner/drafts/{draft_id}
PATCH /api/v1/planner/drafts/{draft_id}
POST /api/v1/planner/drafts/{draft_id}/test
POST /api/v1/planner/drafts/{draft_id}/save
DELETE /api/v1/planner/drafts/{draft_id}
```

### Create Draft Request

```json
{
  "prompt": "Create a customer in the CRM and return the customer ID.",
  "profile": "default",
  "context": {
    "application": "CRM",
    "notes": []
  }
}
```

### Draft Response

Return:

- draft ID
- runner ID/name
- short description
- risk level
- required adapters/capabilities
- inputs
- outputs
- secrets
- steps with editable summaries
- assertions
- open questions
- YAML preview
- policy warnings

## Backend Plan

### Planning Context Builder

Add runtime method that assembles `PlanningContext` from:

- installed adapters
- installed extensions
- existing runners
- security policies
- current desktop observations if available
- LTM examples if present

### Draft Store

Persist GUI drafts in runtime home:

```text
~/.greentic-desktop/gui-drafts/<draft_id>/
  draft.json
  runner.yaml
  request.json
  test-results/
```

This allows browser refresh and process restart recovery.

### Edit Model

Support controlled edits:

- rename runner
- edit description
- add/remove/rename inputs
- add/remove outputs
- mark secrets
- edit step summary/value/locator where safe
- reorder steps if validation can still pass

Do not allow arbitrary YAML text editing as the primary path until schema validation is robust enough.

### Test Flow

Testing should use the existing replay/validation model first:

- validate required capabilities
- resolve sample input values
- simulate or run replay based on available backend mode
- return pass/fail, step traces, outputs, and evidence reference

## Frontend Plan

- Step 1 submits prompt to create a draft.
- Step 2 displays real inputs/outputs and open questions.
- Step 3 displays real steps and YAML preview.
- Step 4 submits sample values to test endpoint.
- Step 5 saves the runner and navigates to `/runners`.
- Use stable loading states; do not advance if backend returns planner diagnostics.

## Acceptance Criteria

- Prompt wizard creates a real runner draft through backend planner APIs.
- The UI shows planner diagnostics and open questions clearly.
- Inputs/outputs/steps are editable and persisted.
- Test runner calls backend validation/replay and displays outputs/evidence.
- Save writes a runner package discoverable by `runner list` and the `/runners` UI.

## Test Plan

- Backend tests for draft create, patch, test, save.
- Frontend route smoke test for prompt wizard happy path.
- Diagnostic test: empty prompt returns `planner.needs_clarification`.
- Policy test: critical/high-risk prompt surfaces approval/policy warning.
- Verify saved runner appears in runtime discovery.

## Risks

- The current heuristic planner may produce limited steps. Keep API shape future-proof for real Greentic LLM responses.
- Editing generated steps can break schema validity; every save must revalidate.

