# PR-41 - Runner Management, Validation, and Publishing

## Goal

Connect the `/runners` page to real runner discovery, validation, test/run execution, editing, and publishing as MCP tools.

## User Outcome

A user can see saved runners, run or test them, fix failed runners, edit metadata, and publish validated runners as MCP tools from the GUI.

## Current State

- `/runners` renders cards from demo data.
- CLI `runner list` discovers local `.gtpack` runners.
- Planner and recorder can write draft runner YAML.
- Replay and MCP crates model validation, replay outcomes, evidence, publishing, and policy.
- Registry and signing models exist but are not exposed as GUI actions.

## Scope

1. Replace demo runner cards with `/api/v1/runners`.
2. Add runner detail/edit route or modal.
3. Add test/run actions with input collection.
4. Add fix/refinement action for failed runners.
5. Add publish-as-MCP action with policy/signing flow.
6. Show evidence and last test results.

## API Design

```http
GET /api/v1/runners
GET /api/v1/runners/{runner_id}
PATCH /api/v1/runners/{runner_id}
POST /api/v1/runners/{runner_id}/validate
POST /api/v1/runners/{runner_id}/test
POST /api/v1/runners/{runner_id}/run
POST /api/v1/runners/{runner_id}/refine
POST /api/v1/runners/{runner_id}/approve
POST /api/v1/runners/{runner_id}/publish
POST /api/v1/runners/{runner_id}/deprecate
```

## Runner Summary Fields

The list endpoint should return:

- ID/name
- description
- version
- lifecycle status: `draft | validated | approved | published | deprecated | failed`
- risk level
- required adapters
- last test status
- updated timestamp
- owner/local user
- MCP publication state
- evidence references

## Backend Plan

### Runner Discovery

Unify CLI `discover_runners` and GUI runner listing around one runtime method:

```rust
DesktopRuntime::list_runner_packages() -> Vec<RunnerSummary>
```

### Validation

Validation should include:

- package parse/schema validity
- required capability availability
- adapter selection
- security policy checks
- required input/secret completeness

### Test/Run

Testing can use sample inputs and replay simulator first. Running may require explicit confirmation for medium/high risk.

Return:

- step traces
- outputs
- failure reason
- evidence bundle URI/reference
- suggested fixes if available

### Publish

Publishing should:

- require a validated runner
- apply security/approval policy
- sign or create registry manifest where applicable
- register as MCP tool
- update runner lifecycle state

## Frontend Plan

- Runner list cards render real data.
- "Run" opens an input modal.
- "Test" opens test modal and result panel.
- "Edit" opens metadata/step editor.
- "Fix" routes to refinement flow.
- "Publish as MCP" asks for confirmation and then calls publish endpoint.
- Cards update after actions.

## Acceptance Criteria

- `/runners` shows saved prompt/recorded runners.
- Test/run actions return real backend results and evidence references.
- Failed runners show fix action and failure details.
- Publishing a runner makes it appear on `/mcp`.
- Policy-denied publish attempts show actionable explanation.

## Test Plan

- Backend tests for list/detail/validate/test/publish.
- Round-trip test: create draft -> save -> list -> test -> publish -> MCP list.
- Frontend smoke tests for runner card actions.
- Security policy tests for high-risk publish flow.

## Risks

- Runner package format may need stronger persisted metadata to support lifecycle state. Prefer adding sidecar metadata files rather than overloading YAML fields until the format is settled.

