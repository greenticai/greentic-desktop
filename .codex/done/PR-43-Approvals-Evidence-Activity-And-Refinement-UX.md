# PR-43 - Approvals, Evidence, Activity, and Refinement UX

## Goal

Expose security approvals, evidence bundles, recent activity, and failed-runner refinement in the GUI so the product supports the full create/test/fix/publish workflow.

## User Outcome

When a test fails or a risky action needs approval, the GUI explains what happened, shows evidence, lets the user approve or reject, and helps refine the runner.

## Current State

- Demo data includes approvals and activity but no route renders approvals directly.
- Evidence crate stores bundles and references.
- Security crate models approval/risk decisions.
- Refinement crate models natural-language corrections and scoped runner diffs.
- LTM crate models failures, corrections, and root-cause summaries.

## Scope

1. Add API endpoints for activity feed.
2. Add API endpoints for evidence bundle summaries and artifacts.
3. Add approval queue endpoints.
4. Add refinement endpoints used by runner fix actions.
5. Add frontend surfaces where current pages need them:
   - home activity feed
   - runner failure details
   - publish approval modal
   - evidence viewer panel
   - fix/refine flow

## API Design

```http
GET /api/v1/activity
GET /api/v1/evidence
GET /api/v1/evidence/{bundle_id}
GET /api/v1/evidence/{bundle_id}/artifacts/{artifact_id}
GET /api/v1/approvals
POST /api/v1/approvals/{approval_id}/approve
POST /api/v1/approvals/{approval_id}/reject
POST /api/v1/runners/{runner_id}/refinement
POST /api/v1/runners/{runner_id}/refinement/{refinement_id}/apply
```

## Backend Plan

### Activity

Normalize telemetry events and high-level lifecycle events into:

- timestamp
- kind: `success | info | warning | error`
- text
- related entity ID
- link target in GUI

### Evidence

Expose evidence bundle metadata:

- run ID
- runner ID
- started/finished timestamps
- inputs hash/redaction summary
- outputs
- screenshots/artifacts
- step traces
- failure reason

Artifact endpoints must enforce local-only access and avoid directory traversal.

### Approvals

Approval records should include:

- action
- runner ID
- risk level
- requested by/local actor
- evidence reference
- policy reason
- status

Approving should write an auditable event.

### Refinement

Use the refinement crate to:

- accept a natural-language correction
- show proposed diff
- apply correction
- rerun validation/test

## Frontend Plan

- Home page can show recent activity below or beside setup checklist.
- Runner cards with `Needs fixing` open a failure/refinement panel.
- Publish high-risk runner opens an approval modal.
- Test/run result cards link to evidence viewer.
- Evidence viewer must redact secrets and show screenshots only through API artifact URLs.

## Acceptance Criteria

- Recent activity is real, not demo data.
- Evidence from runner tests/runs can be opened from the GUI.
- Approval-required actions are blocked until approval is granted.
- Refinement flow previews changes before applying.
- Applied refinements update runner and can be tested again.

## Test Plan

- Evidence API tests for metadata and artifact path safety.
- Approval policy tests.
- Refinement diff/apply tests.
- End-to-end flow: failed test -> refine -> apply -> retest -> evidence.

## Risks

- Evidence may contain sensitive screenshots. Default to loopback-only API, no external URLs, and clear redaction boundaries.

