# PR-75 - Runner Edit Review and Questions UI

## Goal

Build the UI that lets users review, answer questions, adjust, test, and apply LLM-proposed changes to an existing runner.

## User Outcome

After entering an edit instruction, the user sees exactly what will change: inputs, outputs, secrets, steps, assertions, adapters, and output extractors. If the LLM needs clarification, the UI asks targeted questions before applying changes.

## Problem

Editing automations through prompts is risky if the UI hides the diff or silently applies LLM guesses. Users need a controlled review flow that makes changes inspectable and editable.

## Scope

1. Add edit wizard states:
   - describe change
   - answer open questions
   - review structured diff
   - test with sample inputs
   - apply
2. Show current vs proposed fields.
3. Allow safe manual edits before applying.
4. Surface policy warnings and missing adapter/secret requirements.
5. Navigate back to the updated runner after success.

## UI Flow

### Step 1: Describe Change

Show:

- runner name
- current task description
- current inputs/outputs summary
- edit instruction text area
- `Generate Changes`

### Step 2: Open Questions

Only show when planner returns questions.

Question types:

- target application or window
- which output should be returned
- selector/field ambiguity
- credential or secret needed
- unsafe submit confirmation
- adapter installation needed

Answers are persisted and sent back to the patch planner.

### Step 3: Review Changes

Show grouped diffs:

- Runner metadata
- Inputs
- Outputs
- Secrets
- Steps
- Assertions
- Output extractors
- Required adapters
- Policy/risk changes

Use structured before/after rows instead of raw YAML as the primary view. Keep YAML preview available behind an expandable section.

### Step 4: Test Changes

Ask for sample values for the proposed runner inputs. Run the proposed draft through replay/test without replacing the saved runner yet.

Show:

- step trace
- extracted outputs
- evidence refs
- errors
- missing permissions/adapters/secrets

### Step 5: Apply

Apply only after:

- schema validation passes
- base checksum still matches
- required approvals are resolved
- test passed, or the user explicitly accepts a draft-only update when policy allows it

After apply, navigate to the updated runner page/card and show the latest runner state.

## Frontend Plan

- Reuse create wizard components where they are generic.
- Add edit-specific state and copy; do not reuse “Save new runner” language.
- Keep input/output rows editable and persisted in draft state.
- Add clear destructive indicators for removed steps/fields.
- Disable apply while open questions remain.

## Backend Contract Needed

This PR depends on:

- PR-73 edit draft endpoints
- PR-74 patch planner endpoints

Additional endpoints:

```http
POST /api/v1/runners/{runner_id}/edit-drafts/{draft_id}/test
POST /api/v1/runners/{runner_id}/edit-drafts/{draft_id}/apply
```

## Acceptance Criteria

- Edit flow never presents itself as creating a new runner.
- Open questions block apply until answered or intentionally dismissed where safe.
- Users can inspect all LLM-proposed input/output/step/extractor changes.
- Test uses proposed changes and sample inputs, not hard-coded demo values.
- Successful apply returns the user to the runner view and refreshes runner/MCP state.

## Test Plan

- Frontend Playwright test: click runner edit, enter instruction, review generated diff, test, apply.
- Frontend test: open questions are shown and persisted across refresh.
- Frontend test: sample input fields reflect proposed inputs.
- Backend/UI contract test: apply is disabled when schema invalid or questions remain.
- Accessibility test: diff rows and question forms have labels and keyboard navigation.
