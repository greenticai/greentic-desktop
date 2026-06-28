# PR-76 - Apply Edited Runner, Versioning and MCP Refresh

## Goal

Safely apply an edited runner over the existing runner while preserving rollback history and refreshing its automatically exposed MCP tool.

## User Outcome

When a user applies an edit, the existing runner is updated in place, the MCP tool immediately uses the new behavior, and the previous version can be inspected or restored if needed.

## Problem

Prompt edits can break working automations. Applying changes directly to the runner file without version metadata, conflict checks, or rollback makes edits unsafe.

## Scope

1. Add apply endpoint for edit drafts.
2. Add runner version history.
3. Add conflict detection via source checksum.
4. Revalidate schema, policies, secrets, adapters, and replay test results before apply.
5. Refresh runner and MCP views after apply.

## API Design

```http
POST /api/v1/runners/{runner_id}/edit-drafts/{draft_id}/test
POST /api/v1/runners/{runner_id}/edit-drafts/{draft_id}/apply
GET /api/v1/runners/{runner_id}/versions
POST /api/v1/runners/{runner_id}/versions/{version_id}/restore
```

### Apply Response

```json
{
  "runnerId": "calculator.basic",
  "status": "applied",
  "previousVersion": "v3",
  "currentVersion": "v4",
  "mcpTool": "runner.calculator.basic",
  "evidenceRef": "local://runners/calculator.basic/edit/v4"
}
```

## Apply Pipeline

1. Load edit draft.
2. Verify source runner still exists.
3. Verify source checksum matches current runner file.
4. Validate proposed runner schema.
5. Validate adapter requirements and installed extensions.
6. Validate secret references exist or are intentionally unresolved.
7. Run policy checks and approvals.
8. Require successful test unless draft-only apply is explicitly allowed.
9. Write previous runner to history.
10. Atomically replace runner file.
11. Write runner state with edit metadata.
12. Invalidate/reload MCP tool registry.
13. Return updated runner summary and evidence ref.

## Version Store

```text
~/.greentic/desktop/runners/
  calculator.basic.draft.yaml
  calculator.basic.state.json
  versions/
    calculator.basic/
      v1.yaml
      v1.metadata.json
      v2.yaml
      v2.metadata.json
```

Version metadata includes:

- version ID
- created at
- edit instruction
- source checksum
- resulting checksum
- changed fields summary
- test evidence ref
- LLM provider/model used

## MCP Behavior

- Every saved runner remains automatically exposed as MCP.
- Applying an edit updates the MCP tool behavior without a separate publish step.
- MCP tool name remains stable unless the runner ID changes.
- If runner ID changes are allowed later, treat them as migration with explicit warning.

## Conflict Handling

If the runner changed after the edit draft was created:

- return `runner.edit_conflict`
- show current runner summary and draft base summary
- let user rebase the edit through the patch planner
- do not overwrite automatically

## Acceptance Criteria

- Apply updates the existing runner file in place.
- Previous version is persisted before replacement.
- MCP tool list reflects the edited runner immediately.
- Conflict detection prevents overwriting a changed runner.
- Restore can roll back to a previous version.

## Test Plan

- Backend test: apply writes new runner and creates version history.
- Backend test: apply fails on checksum conflict.
- Backend test: MCP tool call uses edited runner after apply.
- Backend test: restore previous version updates runner and MCP behavior.
- Frontend test: apply success navigates to runner view and shows updated fields.
