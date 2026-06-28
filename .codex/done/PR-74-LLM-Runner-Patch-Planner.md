# PR-74 - LLM Runner Patch Planner

## Goal

Use the configured LLM to transform an existing runner plus a user edit instruction into a validated structured patch.

## User Outcome

A user can say things like “also accept multiply as `x`”, “add invoice number as a required input”, or “read the total from the confirmation dialog”, and Greentic proposes precise runner changes with questions when the request is ambiguous.

## Problem

Prompt-to-runner creation can generate a new draft, but existing-runner edits need different behavior. The LLM must preserve working automation steps unless the user asks to change them, produce a minimal diff, and never return ad-hoc JSON or raw YAML that bypasses validation.

## Scope

1. Add a runner patch planning API.
2. Add strict JSON Schema for edit plans.
3. Add repair and retry loops for invalid LLM output.
4. Add open-question handling for ambiguous edits.
5. Keep deterministic fallback behavior for tests when no remote LLM/API key is configured.

## API Design

```http
POST /api/v1/runners/{runner_id}/edit-drafts/{draft_id}/plan
POST /api/v1/runners/{runner_id}/edit-drafts/{draft_id}/answer
```

### Plan Request

```json
{
  "instruction": "Add a discount percentage input and return the discounted total.",
  "answers": {
    "discount_field_location": "Use the app's Discount field if visible."
  }
}
```

### Plan Response

```json
{
  "draftId": "edit-abc",
  "status": "ready",
  "patch": {
    "operations": []
  },
  "proposedRunner": {},
  "openQuestions": [],
  "warnings": [],
  "changeSummary": []
}
```

## Patch Schema

Use serde + JSON Schema for an explicit `RunnerPatchPlan`:

- `intentSummary`
- `preserveBehavior`
- `operations`
- `requiredAdapters`
- `inputChanges`
- `outputChanges`
- `secretChanges`
- `stepChanges`
- `assertionChanges`
- `extractorChanges`
- `policyImpact`
- `openQuestions`

Supported operation types:

- `add_input`
- `update_input`
- `remove_input`
- `add_output`
- `update_output`
- `remove_output`
- `add_secret`
- `add_step`
- `update_step`
- `remove_step`
- `add_assertion`
- `update_assertion`
- `add_output_extractor`
- `update_output_extractor`
- `set_required_adapter`
- `rename_runner`
- `update_description`

Every operation must include:

- stable target path
- human-readable rationale
- before/after values where applicable
- safety classification
- whether replay/test is required before apply

## LLM Prompting Requirements

The LLM receives:

- existing runner schema, not just YAML text
- current inputs, outputs, secrets, steps, assertions, adapters
- evidence from latest successful run if available
- user edit instruction
- installed adapter capabilities
- policy constraints
- strict JSON Schema

The LLM must:

- return only JSON matching `RunnerPatchPlan`
- preserve existing behavior unless explicitly changed
- ask questions instead of guessing missing app/output/credential details
- prefer adding typed inputs/outputs/extractors over sample placeholders
- avoid company/email defaults or CRM-specific assumptions

## Repair Loop

On invalid LLM output:

1. Strip markdown fences and control characters.
2. Parse as JSON.
3. Validate against schema.
4. If validation fails, send a repair prompt containing the validation errors and original output.
5. Retry with a bounded count.
6. Return `llm.invalid_patch_json` with diagnostics if still invalid.

## Backend Plan

- Add `RunnerPatchPlanner` to planner or runner-schema domain.
- Add schema export for `RunnerPatchPlan`.
- Add deterministic heuristic patcher for test mode.
- Apply patch to an in-memory runner model and validate the proposed model before returning.
- Store plan attempts and diagnostics in the edit draft directory.

## Acceptance Criteria

- Edit planning uses configured LLM provider when available.
- Missing API key returns a settings/secrets action, not an empty plan.
- Invalid LLM JSON is repaired or returned as structured diagnostics.
- The patch planner returns open questions instead of hallucinated fields.
- Proposed runner validates against the canonical runner schema before the UI can proceed.

## Test Plan

- Unit tests for patch schema serialization and validation.
- Unit tests for patch operations applied to runner models.
- LLM mock tests for valid response, fenced JSON, control characters, invalid schema, and repair.
- Backend integration test: calculator runner edit adds a new input/output without creating a new runner.
- Backend integration test: ambiguous instruction returns open questions.
