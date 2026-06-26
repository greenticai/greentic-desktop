# PR-72 - Prompt-Based Runner Update and Diff Apply

## Goal

Implement reliable LLM-backed updates to existing runners from natural-language prompts, with structured patches, validation, preview, replay/test, and safe apply.

## Problem

The current runner refinement flow is a placeholder. The backend does not load and patch the actual runner, and the refinement crate uses keyword parsing instead of the configured LLM. The GUI can say "Preview fix" and "Apply fix," but the diff does not represent real runner changes.

## User Outcome

A user can open an existing runner, describe a change, preview the exact proposed update, answer questions if needed, test it, and apply it. Examples:

- "Add phone number as an input and fill it before saving."
- "The Save button is now called Submit."
- "Return the invoice number as an output."
- "Use the terminal runner instead of the web flow."
- "Remove the approval step."

## Design

Add a structured update operation:

```rust
pub struct RunnerUpdateRequest {
    pub runner_id: String,
    pub user_prompt: String,
    pub current_runner: RunnerDefinition,
    pub current_requirements: Option<RunnerRequirements>,
    pub last_run_evidence: Option<EvidenceSummary>,
    pub available_capabilities: Vec<AdapterCapabilities>,
}

pub struct RunnerUpdatePlan {
    pub summary: String,
    pub questions: Vec<PlannerQuestion>,
    pub operations: Vec<RunnerPatchOperation>,
    pub expected_effects: Vec<String>,
    pub risks: Vec<String>,
    pub requires_test: bool,
}
```

Patch operations should be structured and limited:

- add/update/remove input
- add/update/remove secret
- add/update/remove output extractor
- add/update/remove workflow action
- update locator
- update assertion
- update risk/approval policy
- update target technology
- update requirement/assumption

## Apply Pipeline

```text
load runner
  -> parse prompt and context through LLM strict schema
  -> validate patch operations
  -> apply to in-memory RunnerDefinition/DesktopWorkflow
  -> compile
  -> policy/capability validate
  -> produce diff preview
  -> require user approval
  -> run test/replay if requested
  -> write new runner version
  -> update MCP tool manifest atomically
```

## Diff Format

Store and return a semantic diff:

- added/removed/changed inputs
- added/removed/changed outputs
- changed steps/actions
- changed locators
- changed risk/approval
- YAML before/after for advanced view

Avoid plain text-only diffs as the primary API. They are hard for the GUI to reason about.

## Backend API

Add:

```http
POST /api/v1/runners/{id}/updates
GET /api/v1/runners/{id}/updates/{update_id}
POST /api/v1/runners/{id}/updates/{update_id}/answer
POST /api/v1/runners/{id}/updates/{update_id}/test
POST /api/v1/runners/{id}/updates/{update_id}/apply
POST /api/v1/runners/{id}/updates/{update_id}/discard
```

Keep `/refinement` as an alias during migration.

## Persistence

Store under runtime home:

- `runner-updates/{runner_id}/{update_id}/request.json`
- `plan.json`
- `patch.json`
- `diff.json`
- `candidate.runner.json`
- `candidate.runner.yaml`
- `test-result.json`
- `trace.json`

## Safety

- Do not apply updates that fail schema/compile/policy validation.
- High-risk or destructive changes require explicit confirmation.
- If the update removes outputs or inputs, call that out in the preview.
- Preserve previous runner file/version until apply succeeds.
- Update MCP tool atomically after runner apply.

## Acceptance Criteria

- Prompt update loads the actual runner and returns a real semantic diff.
- Adding an input persists into runner input schema and MCP tool input schema.
- Adding an output persists into runner output schema and run result extraction.
- Locator update changes the target in the workflow/compiled steps.
- Apply writes a new runner version and leaves prior runner recoverable.
- Invalid update prompts ask questions or fail with structured diagnostics.

## Test Plan

- Add input via prompt, preview diff, apply, verify runner YAML and MCP schema.
- Add output extractor via prompt, test, apply, verify output schema.
- Rename locator via prompt and verify compiled step target changes.
- Attempt policy-denied update and verify it cannot apply.
- Apply failure test proves original runner file remains unchanged.
- GUI mocked-flow test for preview, questions, test, apply, discard.

## Risks

- YAML-only runners may not deserialize into full `RunnerDefinition` yet. Add a compatibility loader or migrate saved prompt runners to the schema-backed format.
- LLM patches can be over-broad. Restrict operation enum and validate every operation against existing IDs.

