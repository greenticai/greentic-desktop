# PR-147 - Runner YAML Export And Download From My Runners

## Goal

Allow users to export a runner YAML from "My runners" and download it locally.

## User Outcome

A user can share, back up, review, or move a runner by downloading its canonical YAML from the runner page/list without using the CLI.

## Current Evidence

- The CLI already supports `--export (PATH|ID) --out PATH` and `runner export`.
- The GUI runner list supports run, edit, rename, and delete flows, but no YAML export/download action.
- The product now needs a complete import/export loop for runner portability.

## Scope

1. Add backend export endpoint:
   - `GET /api/v1/runners/{id}/yaml`
   - returns `Content-Type: application/x-yaml; charset=utf-8`.
   - sets `Content-Disposition: attachment; filename="<safe-runner-id>.yaml"`.
2. Add optional metadata endpoint/action if needed:
   - `POST /api/v1/runners/{id}/export`
   - returns JSON with filename and download URL.
   - only add this if the frontend cannot cleanly download from `GET`.
3. Use canonical YAML:
   - generated from the typed runner model.
   - includes inputs, outputs, secrets declarations, assertions, target, steps, and metadata.
   - does not include evidence, secret values, runtime state, API keys, or local execution logs.
4. Add UI action in My runners:
   - menu/button: `Export YAML` or download icon.
   - available from runner row/card.
   - available from runner detail page if such view exists.
5. Keep delete all-or-nothing semantics:
   - export is read-only and must not affect MCP publishing/running state.
6. Filename safety:
   - use existing safe runner filename helper.
   - avoid path separators and unsafe characters.

## File Targets

- `crates/greentic-desktop-gui/src/lib.rs`
- `frontend/automate-hub/src/routes/runners.tsx`
- `frontend/automate-hub/src/lib/api.ts`
- `frontend/automate-hub/src/lib/types.ts`

## Out of Scope

- Exporting evidence bundles.
- Exporting `.gtpack` packages.
- Signing exported runner YAML.

## Acceptance Tests

1. `GET /api/v1/runners/{id}/yaml` returns a YAML attachment for an existing runner.
2. The YAML can be re-imported through the new GUI import endpoint.
3. The exported YAML contains no secret values.
4. The My runners page has an export/download action for every runner.
5. Clicking export downloads a `.yaml` file without navigating away or deleting/changing the runner.
6. Missing runner id returns a structured 404.

## Done Means

Runner portability is complete in the GUI: users can import YAML and export/download YAML without touching the CLI.
