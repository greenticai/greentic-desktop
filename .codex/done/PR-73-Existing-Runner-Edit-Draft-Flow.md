# PR-73 - Existing Runner Edit Draft Flow

## Goal

Make the `Edit` action on a runner open an edit flow for that runner, not the new-runner prompt wizard.

## User Outcome

A user can click `Edit` on `My Runners`, see the current runner task preloaded, describe how they want to extend or change it, and continue through a runner-edit wizard without creating a separate new runner.

## Problem

The current edit button routes to `/create`, which starts from an empty prompt flow. This loses the existing runner context and encourages duplicate runners instead of controlled evolution of the saved runner.

## Scope

1. Add a runner edit route or mode:
   - `/runners/{runner_id}/edit`, or
   - `/create?mode=edit&runner=<runner_id>`
2. Add a backend runner detail endpoint suitable for editing:
   - runner summary
   - current YAML/schema model
   - original prompt or inferred task description
   - inputs, outputs, secrets, steps, assertions, adapters, policy metadata
   - current version/checksum
3. Add an edit draft store separate from create drafts.
4. Make edit drafts reference the source runner and base checksum.
5. Route `Edit` buttons to the edit draft flow.

## API Design

```http
GET /api/v1/runners/{runner_id}
POST /api/v1/runners/{runner_id}/edit-drafts
GET /api/v1/runners/{runner_id}/edit-drafts/{draft_id}
PATCH /api/v1/runners/{runner_id}/edit-drafts/{draft_id}
DELETE /api/v1/runners/{runner_id}/edit-drafts/{draft_id}
```

### Create Edit Draft Request

```json
{
  "instruction": "Also support percentage calculations and return the final displayed value.",
  "mode": "extend"
}
```

### Edit Draft Response

Return:

- `draftId`
- `sourceRunnerId`
- `sourceChecksum`
- `instruction`
- parsed existing runner model
- proposed runner model, initially equal to the source
- open questions
- warnings
- change summary
- YAML preview

## Backend Plan

- Add a typed `RunnerEditDraft` struct in the GUI/API layer or runner-schema crate.
- Parse existing runner YAML into the canonical runner schema instead of passing raw YAML through the UI.
- Store edit drafts under:

```text
~/.greentic/desktop/gui-edit-drafts/<runner_id>/<draft_id>/
  request.json
  source.runner.json
  proposed.runner.json
  source.yaml
  proposed.yaml
  metadata.json
```

- Include `sourceChecksum` so apply can detect if the runner changed while the edit draft was open.
- Return a conflict if the saved runner no longer matches the draft base.

## Frontend Plan

- Update runner cards so `Edit` navigates with the runner ID.
- Render a first edit screen with:
  - current runner name and description
  - current task prompt/description preloaded
  - a text area asking what to change or extend
  - a clear `Generate Changes` action
- Do not show the new-runner first step for existing runners.
- Keep the edit flow visually close to the create flow, but label it as editing the selected runner.

## Acceptance Criteria

- Clicking `Edit` on any runner opens an edit flow with that runner loaded.
- The user can enter an edit instruction against the existing runner.
- Refreshing the browser can recover the edit draft.
- Edit drafts preserve source runner checksum and current schema fields.
- No duplicate runner is created by entering the edit flow.

## Test Plan

- Backend test: `GET /api/v1/runners/{id}` returns editable schema data.
- Backend test: create edit draft persists source and proposed model.
- Backend test: missing runner returns `runner.not_found`.
- Frontend test: runner `Edit` opens the edit route and shows the runner name/current description.
- Frontend test: creating an edit draft does not add a new runner to `/api/v1/runners`.
